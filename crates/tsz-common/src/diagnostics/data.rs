//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY — run `node scripts/gen_diagnostics.mjs` to regenerate.
use super::DiagnosticCategory;
use super::DiagnosticMessage;

/// All diagnostic messages from TypeScript's diagnosticMessages.json.
pub static DIAGNOSTIC_MESSAGES: &[DiagnosticMessage] = &[
    DiagnosticMessage {
        code: 1002,
        category: DiagnosticCategory::Error,
        message: "Unterminated string literal.",
    },
    DiagnosticMessage {
        code: 1003,
        category: DiagnosticCategory::Error,
        message: "Identifier expected.",
    },
    DiagnosticMessage {
        code: 1005,
        category: DiagnosticCategory::Error,
        message: "'{0}' expected.",
    },
    DiagnosticMessage {
        code: 1006,
        category: DiagnosticCategory::Error,
        message: "A file cannot have a reference to itself.",
    },
    DiagnosticMessage {
        code: 1007,
        category: DiagnosticCategory::Error,
        message: "The parser expected to find a '{1}' to match the '{0}' token here.",
    },
    DiagnosticMessage {
        code: 1009,
        category: DiagnosticCategory::Error,
        message: "Trailing comma not allowed.",
    },
    DiagnosticMessage {
        code: 1010,
        category: DiagnosticCategory::Error,
        message: "'*/' expected.",
    },
    DiagnosticMessage {
        code: 1011,
        category: DiagnosticCategory::Error,
        message: "An element access expression should take an argument.",
    },
    DiagnosticMessage {
        code: 1012,
        category: DiagnosticCategory::Error,
        message: "Unexpected token.",
    },
    DiagnosticMessage {
        code: 1013,
        category: DiagnosticCategory::Error,
        message: "A rest parameter or binding pattern may not have a trailing comma.",
    },
    DiagnosticMessage {
        code: 1014,
        category: DiagnosticCategory::Error,
        message: "A rest parameter must be last in a parameter list.",
    },
    DiagnosticMessage {
        code: 1015,
        category: DiagnosticCategory::Error,
        message: "Parameter cannot have question mark and initializer.",
    },
    DiagnosticMessage {
        code: 1016,
        category: DiagnosticCategory::Error,
        message: "A required parameter cannot follow an optional parameter.",
    },
    DiagnosticMessage {
        code: 1017,
        category: DiagnosticCategory::Error,
        message: "An index signature cannot have a rest parameter.",
    },
    DiagnosticMessage {
        code: 1018,
        category: DiagnosticCategory::Error,
        message: "An index signature parameter cannot have an accessibility modifier.",
    },
    DiagnosticMessage {
        code: 1019,
        category: DiagnosticCategory::Error,
        message: "An index signature parameter cannot have a question mark.",
    },
    DiagnosticMessage {
        code: 1020,
        category: DiagnosticCategory::Error,
        message: "An index signature parameter cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1021,
        category: DiagnosticCategory::Error,
        message: "An index signature must have a type annotation.",
    },
    DiagnosticMessage {
        code: 1022,
        category: DiagnosticCategory::Error,
        message: "An index signature parameter must have a type annotation.",
    },
    DiagnosticMessage {
        code: 1024,
        category: DiagnosticCategory::Error,
        message: "'readonly' modifier can only appear on a property declaration or index signature.",
    },
    DiagnosticMessage {
        code: 1025,
        category: DiagnosticCategory::Error,
        message: "An index signature cannot have a trailing comma.",
    },
    DiagnosticMessage {
        code: 1028,
        category: DiagnosticCategory::Error,
        message: "Accessibility modifier already seen.",
    },
    DiagnosticMessage {
        code: 1029,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier must precede '{1}' modifier.",
    },
    DiagnosticMessage {
        code: 1030,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier already seen.",
    },
    DiagnosticMessage {
        code: 1031,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on class elements of this kind.",
    },
    DiagnosticMessage {
        code: 1034,
        category: DiagnosticCategory::Error,
        message: "'super' must be followed by an argument list or member access.",
    },
    DiagnosticMessage {
        code: 1035,
        category: DiagnosticCategory::Error,
        message: "Only ambient modules can use quoted names.",
    },
    DiagnosticMessage {
        code: 1036,
        category: DiagnosticCategory::Error,
        message: "Statements are not allowed in ambient contexts.",
    },
    DiagnosticMessage {
        code: 1038,
        category: DiagnosticCategory::Error,
        message: "A 'declare' modifier cannot be used in an already ambient context.",
    },
    DiagnosticMessage {
        code: 1039,
        category: DiagnosticCategory::Error,
        message: "Initializers are not allowed in ambient contexts.",
    },
    DiagnosticMessage {
        code: 1040,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot be used in an ambient context.",
    },
    DiagnosticMessage {
        code: 1042,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot be used here.",
    },
    DiagnosticMessage {
        code: 1044,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on a module or namespace element.",
    },
    DiagnosticMessage {
        code: 1046,
        category: DiagnosticCategory::Error,
        message: "Top-level declarations in .d.ts files must start with either a 'declare' or 'export' modifier.",
    },
    DiagnosticMessage {
        code: 1047,
        category: DiagnosticCategory::Error,
        message: "A rest parameter cannot be optional.",
    },
    DiagnosticMessage {
        code: 1048,
        category: DiagnosticCategory::Error,
        message: "A rest parameter cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1049,
        category: DiagnosticCategory::Error,
        message: "A 'set' accessor must have exactly one parameter.",
    },
    DiagnosticMessage {
        code: 1051,
        category: DiagnosticCategory::Error,
        message: "A 'set' accessor cannot have an optional parameter.",
    },
    DiagnosticMessage {
        code: 1052,
        category: DiagnosticCategory::Error,
        message: "A 'set' accessor parameter cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1053,
        category: DiagnosticCategory::Error,
        message: "A 'set' accessor cannot have rest parameter.",
    },
    DiagnosticMessage {
        code: 1054,
        category: DiagnosticCategory::Error,
        message: "A 'get' accessor cannot have parameters.",
    },
    DiagnosticMessage {
        code: 1055,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not a valid async function return type in ES5 because it does not refer to a Promise-compatible constructor value.",
    },
    DiagnosticMessage {
        code: 1056,
        category: DiagnosticCategory::Error,
        message: "Accessors are only available when targeting ECMAScript 5 and higher.",
    },
    DiagnosticMessage {
        code: 1058,
        category: DiagnosticCategory::Error,
        message: "The return type of an async function must either be a valid promise or must not contain a callable 'then' member.",
    },
    DiagnosticMessage {
        code: 1059,
        category: DiagnosticCategory::Error,
        message: "A promise must have a 'then' method.",
    },
    DiagnosticMessage {
        code: 1060,
        category: DiagnosticCategory::Error,
        message: "The first parameter of the 'then' method of a promise must be a callback.",
    },
    DiagnosticMessage {
        code: 1061,
        category: DiagnosticCategory::Error,
        message: "Enum member must have initializer.",
    },
    DiagnosticMessage {
        code: 1062,
        category: DiagnosticCategory::Error,
        message: "Type is referenced directly or indirectly in the fulfillment callback of its own 'then' method.",
    },
    DiagnosticMessage {
        code: 1063,
        category: DiagnosticCategory::Error,
        message: "An export assignment cannot be used in a namespace.",
    },
    DiagnosticMessage {
        code: 1064,
        category: DiagnosticCategory::Error,
        message: "The return type of an async function or method must be the global Promise<T> type. Did you mean to write 'Promise<{0}>'?",
    },
    DiagnosticMessage {
        code: 1065,
        category: DiagnosticCategory::Error,
        message: "The return type of an async function or method must be the global Promise<T> type.",
    },
    DiagnosticMessage {
        code: 1066,
        category: DiagnosticCategory::Error,
        message: "In ambient enum declarations member initializer must be constant expression.",
    },
    DiagnosticMessage {
        code: 1068,
        category: DiagnosticCategory::Error,
        message: "Unexpected token. A constructor, method, accessor, or property was expected.",
    },
    DiagnosticMessage {
        code: 1069,
        category: DiagnosticCategory::Error,
        message: "Unexpected token. A type parameter name was expected without curly braces.",
    },
    DiagnosticMessage {
        code: 1070,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on a type member.",
    },
    DiagnosticMessage {
        code: 1071,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on an index signature.",
    },
    DiagnosticMessage {
        code: 1079,
        category: DiagnosticCategory::Error,
        message: "A '{0}' modifier cannot be used with an import declaration.",
    },
    DiagnosticMessage {
        code: 1084,
        category: DiagnosticCategory::Error,
        message: "Invalid 'reference' directive syntax.",
    },
    DiagnosticMessage {
        code: 1089,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on a constructor declaration.",
    },
    DiagnosticMessage {
        code: 1090,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on a parameter.",
    },
    DiagnosticMessage {
        code: 1091,
        category: DiagnosticCategory::Error,
        message: "Only a single variable declaration is allowed in a 'for...in' statement.",
    },
    DiagnosticMessage {
        code: 1092,
        category: DiagnosticCategory::Error,
        message: "Type parameters cannot appear on a constructor declaration.",
    },
    DiagnosticMessage {
        code: 1093,
        category: DiagnosticCategory::Error,
        message: "Type annotation cannot appear on a constructor declaration.",
    },
    DiagnosticMessage {
        code: 1094,
        category: DiagnosticCategory::Error,
        message: "An accessor cannot have type parameters.",
    },
    DiagnosticMessage {
        code: 1095,
        category: DiagnosticCategory::Error,
        message: "A 'set' accessor cannot have a return type annotation.",
    },
    DiagnosticMessage {
        code: 1096,
        category: DiagnosticCategory::Error,
        message: "An index signature must have exactly one parameter.",
    },
    DiagnosticMessage {
        code: 1097,
        category: DiagnosticCategory::Error,
        message: "'{0}' list cannot be empty.",
    },
    DiagnosticMessage {
        code: 1098,
        category: DiagnosticCategory::Error,
        message: "Type parameter list cannot be empty.",
    },
    DiagnosticMessage {
        code: 1099,
        category: DiagnosticCategory::Error,
        message: "Type argument list cannot be empty.",
    },
    DiagnosticMessage {
        code: 1100,
        category: DiagnosticCategory::Error,
        message: "Invalid use of '{0}' in strict mode.",
    },
    DiagnosticMessage {
        code: 1101,
        category: DiagnosticCategory::Error,
        message: "'with' statements are not allowed in strict mode.",
    },
    DiagnosticMessage {
        code: 1102,
        category: DiagnosticCategory::Error,
        message: "'delete' cannot be called on an identifier in strict mode.",
    },
    DiagnosticMessage {
        code: 1103,
        category: DiagnosticCategory::Error,
        message: "'for await' loops are only allowed within async functions and at the top levels of modules.",
    },
    DiagnosticMessage {
        code: 1104,
        category: DiagnosticCategory::Error,
        message: "A 'continue' statement can only be used within an enclosing iteration statement.",
    },
    DiagnosticMessage {
        code: 1105,
        category: DiagnosticCategory::Error,
        message: "A 'break' statement can only be used within an enclosing iteration or switch statement.",
    },
    DiagnosticMessage {
        code: 1106,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...of' statement may not be 'async'.",
    },
    DiagnosticMessage {
        code: 1107,
        category: DiagnosticCategory::Error,
        message: "Jump target cannot cross function boundary.",
    },
    DiagnosticMessage {
        code: 1108,
        category: DiagnosticCategory::Error,
        message: "A 'return' statement can only be used within a function body.",
    },
    DiagnosticMessage {
        code: 1109,
        category: DiagnosticCategory::Error,
        message: "Expression expected.",
    },
    DiagnosticMessage {
        code: 1110,
        category: DiagnosticCategory::Error,
        message: "Type expected.",
    },
    DiagnosticMessage {
        code: 1111,
        category: DiagnosticCategory::Error,
        message: "Private field '{0}' must be declared in an enclosing class.",
    },
    DiagnosticMessage {
        code: 1113,
        category: DiagnosticCategory::Error,
        message: "A 'default' clause cannot appear more than once in a 'switch' statement.",
    },
    DiagnosticMessage {
        code: 1114,
        category: DiagnosticCategory::Error,
        message: "Duplicate label '{0}'.",
    },
    DiagnosticMessage {
        code: 1115,
        category: DiagnosticCategory::Error,
        message: "A 'continue' statement can only jump to a label of an enclosing iteration statement.",
    },
    DiagnosticMessage {
        code: 1116,
        category: DiagnosticCategory::Error,
        message: "A 'break' statement can only jump to a label of an enclosing statement.",
    },
    DiagnosticMessage {
        code: 1117,
        category: DiagnosticCategory::Error,
        message: "An object literal cannot have multiple properties with the same name.",
    },
    DiagnosticMessage {
        code: 1118,
        category: DiagnosticCategory::Error,
        message: "An object literal cannot have multiple get/set accessors with the same name.",
    },
    DiagnosticMessage {
        code: 1119,
        category: DiagnosticCategory::Error,
        message: "An object literal cannot have property and accessor with the same name.",
    },
    DiagnosticMessage {
        code: 1120,
        category: DiagnosticCategory::Error,
        message: "An export assignment cannot have modifiers.",
    },
    DiagnosticMessage {
        code: 1121,
        category: DiagnosticCategory::Error,
        message: "Octal literals are not allowed. Use the syntax '{0}'.",
    },
    DiagnosticMessage {
        code: 1123,
        category: DiagnosticCategory::Error,
        message: "Variable declaration list cannot be empty.",
    },
    DiagnosticMessage {
        code: 1124,
        category: DiagnosticCategory::Error,
        message: "Digit expected.",
    },
    DiagnosticMessage {
        code: 1125,
        category: DiagnosticCategory::Error,
        message: "Hexadecimal digit expected.",
    },
    DiagnosticMessage {
        code: 1126,
        category: DiagnosticCategory::Error,
        message: "Unexpected end of text.",
    },
    DiagnosticMessage {
        code: 1127,
        category: DiagnosticCategory::Error,
        message: "Invalid character.",
    },
    DiagnosticMessage {
        code: 1128,
        category: DiagnosticCategory::Error,
        message: "Declaration or statement expected.",
    },
    DiagnosticMessage {
        code: 1129,
        category: DiagnosticCategory::Error,
        message: "Statement expected.",
    },
    DiagnosticMessage {
        code: 1130,
        category: DiagnosticCategory::Error,
        message: "'case' or 'default' expected.",
    },
    DiagnosticMessage {
        code: 1131,
        category: DiagnosticCategory::Error,
        message: "Property or signature expected.",
    },
    DiagnosticMessage {
        code: 1132,
        category: DiagnosticCategory::Error,
        message: "Enum member expected.",
    },
    DiagnosticMessage {
        code: 1134,
        category: DiagnosticCategory::Error,
        message: "Variable declaration expected.",
    },
    DiagnosticMessage {
        code: 1135,
        category: DiagnosticCategory::Error,
        message: "Argument expression expected.",
    },
    DiagnosticMessage {
        code: 1136,
        category: DiagnosticCategory::Error,
        message: "Property assignment expected.",
    },
    DiagnosticMessage {
        code: 1137,
        category: DiagnosticCategory::Error,
        message: "Expression or comma expected.",
    },
    DiagnosticMessage {
        code: 1138,
        category: DiagnosticCategory::Error,
        message: "Parameter declaration expected.",
    },
    DiagnosticMessage {
        code: 1139,
        category: DiagnosticCategory::Error,
        message: "Type parameter declaration expected.",
    },
    DiagnosticMessage {
        code: 1140,
        category: DiagnosticCategory::Error,
        message: "Type argument expected.",
    },
    DiagnosticMessage {
        code: 1141,
        category: DiagnosticCategory::Error,
        message: "String literal expected.",
    },
    DiagnosticMessage {
        code: 1142,
        category: DiagnosticCategory::Error,
        message: "Line break not permitted here.",
    },
    DiagnosticMessage {
        code: 1144,
        category: DiagnosticCategory::Error,
        message: "'{' or ';' expected.",
    },
    DiagnosticMessage {
        code: 1145,
        category: DiagnosticCategory::Error,
        message: "'{' or JSX element expected.",
    },
    DiagnosticMessage {
        code: 1146,
        category: DiagnosticCategory::Error,
        message: "Declaration expected.",
    },
    DiagnosticMessage {
        code: 1147,
        category: DiagnosticCategory::Error,
        message: "Import declarations in a namespace cannot reference a module.",
    },
    DiagnosticMessage {
        code: 1148,
        category: DiagnosticCategory::Error,
        message: "Cannot use imports, exports, or module augmentations when '--module' is 'none'.",
    },
    DiagnosticMessage {
        code: 1149,
        category: DiagnosticCategory::Error,
        message: "File name '{0}' differs from already included file name '{1}' only in casing.",
    },
    DiagnosticMessage {
        code: 1155,
        category: DiagnosticCategory::Error,
        message: "'{0}' declarations must be initialized.",
    },
    DiagnosticMessage {
        code: 1156,
        category: DiagnosticCategory::Error,
        message: "'{0}' declarations can only be declared inside a block.",
    },
    DiagnosticMessage {
        code: 1160,
        category: DiagnosticCategory::Error,
        message: "Unterminated template literal.",
    },
    DiagnosticMessage {
        code: 1161,
        category: DiagnosticCategory::Error,
        message: "Unterminated regular expression literal.",
    },
    DiagnosticMessage {
        code: 1162,
        category: DiagnosticCategory::Error,
        message: "An object member cannot be declared optional.",
    },
    DiagnosticMessage {
        code: 1163,
        category: DiagnosticCategory::Error,
        message: "A 'yield' expression is only allowed in a generator body.",
    },
    DiagnosticMessage {
        code: 1164,
        category: DiagnosticCategory::Error,
        message: "Computed property names are not allowed in enums.",
    },
    DiagnosticMessage {
        code: 1165,
        category: DiagnosticCategory::Error,
        message: "A computed property name in an ambient context must refer to an expression whose type is a literal type or a 'unique symbol' type.",
    },
    DiagnosticMessage {
        code: 1166,
        category: DiagnosticCategory::Error,
        message: "A computed property name in a class property declaration must have a simple literal type or a 'unique symbol' type.",
    },
    DiagnosticMessage {
        code: 1168,
        category: DiagnosticCategory::Error,
        message: "A computed property name in a method overload must refer to an expression whose type is a literal type or a 'unique symbol' type.",
    },
    DiagnosticMessage {
        code: 1169,
        category: DiagnosticCategory::Error,
        message: "A computed property name in an interface must refer to an expression whose type is a literal type or a 'unique symbol' type.",
    },
    DiagnosticMessage {
        code: 1170,
        category: DiagnosticCategory::Error,
        message: "A computed property name in a type literal must refer to an expression whose type is a literal type or a 'unique symbol' type.",
    },
    DiagnosticMessage {
        code: 1171,
        category: DiagnosticCategory::Error,
        message: "A comma expression is not allowed in a computed property name.",
    },
    DiagnosticMessage {
        code: 1172,
        category: DiagnosticCategory::Error,
        message: "'extends' clause already seen.",
    },
    DiagnosticMessage {
        code: 1173,
        category: DiagnosticCategory::Error,
        message: "'extends' clause must precede 'implements' clause.",
    },
    DiagnosticMessage {
        code: 1174,
        category: DiagnosticCategory::Error,
        message: "Classes can only extend a single class.",
    },
    DiagnosticMessage {
        code: 1175,
        category: DiagnosticCategory::Error,
        message: "'implements' clause already seen.",
    },
    DiagnosticMessage {
        code: 1176,
        category: DiagnosticCategory::Error,
        message: "Interface declaration cannot have 'implements' clause.",
    },
    DiagnosticMessage {
        code: 1177,
        category: DiagnosticCategory::Error,
        message: "Binary digit expected.",
    },
    DiagnosticMessage {
        code: 1178,
        category: DiagnosticCategory::Error,
        message: "Octal digit expected.",
    },
    DiagnosticMessage {
        code: 1179,
        category: DiagnosticCategory::Error,
        message: "Unexpected token. '{' expected.",
    },
    DiagnosticMessage {
        code: 1180,
        category: DiagnosticCategory::Error,
        message: "Property destructuring pattern expected.",
    },
    DiagnosticMessage {
        code: 1181,
        category: DiagnosticCategory::Error,
        message: "Array element destructuring pattern expected.",
    },
    DiagnosticMessage {
        code: 1182,
        category: DiagnosticCategory::Error,
        message: "A destructuring declaration must have an initializer.",
    },
    DiagnosticMessage {
        code: 1183,
        category: DiagnosticCategory::Error,
        message: "An implementation cannot be declared in ambient contexts.",
    },
    DiagnosticMessage {
        code: 1184,
        category: DiagnosticCategory::Error,
        message: "Modifiers cannot appear here.",
    },
    DiagnosticMessage {
        code: 1185,
        category: DiagnosticCategory::Error,
        message: "Merge conflict marker encountered.",
    },
    DiagnosticMessage {
        code: 1186,
        category: DiagnosticCategory::Error,
        message: "A rest element cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1187,
        category: DiagnosticCategory::Error,
        message: "A parameter property may not be declared using a binding pattern.",
    },
    DiagnosticMessage {
        code: 1188,
        category: DiagnosticCategory::Error,
        message: "Only a single variable declaration is allowed in a 'for...of' statement.",
    },
    DiagnosticMessage {
        code: 1189,
        category: DiagnosticCategory::Error,
        message: "The variable declaration of a 'for...in' statement cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1190,
        category: DiagnosticCategory::Error,
        message: "The variable declaration of a 'for...of' statement cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1191,
        category: DiagnosticCategory::Error,
        message: "An import declaration cannot have modifiers.",
    },
    DiagnosticMessage {
        code: 1192,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' has no default export.",
    },
    DiagnosticMessage {
        code: 1193,
        category: DiagnosticCategory::Error,
        message: "An export declaration cannot have modifiers.",
    },
    DiagnosticMessage {
        code: 1194,
        category: DiagnosticCategory::Error,
        message: "Export declarations are not permitted in a namespace.",
    },
    DiagnosticMessage {
        code: 1195,
        category: DiagnosticCategory::Error,
        message: "'export *' does not re-export a default.",
    },
    DiagnosticMessage {
        code: 1196,
        category: DiagnosticCategory::Error,
        message: "Catch clause variable type annotation must be 'any' or 'unknown' if specified.",
    },
    DiagnosticMessage {
        code: 1197,
        category: DiagnosticCategory::Error,
        message: "Catch clause variable cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1198,
        category: DiagnosticCategory::Error,
        message: "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.",
    },
    DiagnosticMessage {
        code: 1199,
        category: DiagnosticCategory::Error,
        message: "Unterminated Unicode escape sequence.",
    },
    DiagnosticMessage {
        code: 1200,
        category: DiagnosticCategory::Error,
        message: "Line terminator not permitted before arrow.",
    },
    DiagnosticMessage {
        code: 1202,
        category: DiagnosticCategory::Error,
        message: "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.",
    },
    DiagnosticMessage {
        code: 1203,
        category: DiagnosticCategory::Error,
        message: "Export assignment cannot be used when targeting ECMAScript modules. Consider using 'export default' or another module format instead.",
    },
    DiagnosticMessage {
        code: 1205,
        category: DiagnosticCategory::Error,
        message: "Re-exporting a type when '{0}' is enabled requires using 'export type'.",
    },
    DiagnosticMessage {
        code: 1206,
        category: DiagnosticCategory::Error,
        message: "Decorators are not valid here.",
    },
    DiagnosticMessage {
        code: 1207,
        category: DiagnosticCategory::Error,
        message: "Decorators cannot be applied to multiple get/set accessors of the same name.",
    },
    DiagnosticMessage {
        code: 1209,
        category: DiagnosticCategory::Error,
        message: "Invalid optional chain from new expression. Did you mean to call '{0}()'?",
    },
    DiagnosticMessage {
        code: 1210,
        category: DiagnosticCategory::Error,
        message: "Code contained in a class is evaluated in JavaScript's strict mode which does not allow this use of '{0}'. For more information, see https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Strict_mode.",
    },
    DiagnosticMessage {
        code: 1211,
        category: DiagnosticCategory::Error,
        message: "A class declaration without the 'default' modifier must have a name.",
    },
    DiagnosticMessage {
        code: 1212,
        category: DiagnosticCategory::Error,
        message: "Identifier expected. '{0}' is a reserved word in strict mode.",
    },
    DiagnosticMessage {
        code: 1213,
        category: DiagnosticCategory::Error,
        message: "Identifier expected. '{0}' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
    },
    DiagnosticMessage {
        code: 1214,
        category: DiagnosticCategory::Error,
        message: "Identifier expected. '{0}' is a reserved word in strict mode. Modules are automatically in strict mode.",
    },
    DiagnosticMessage {
        code: 1215,
        category: DiagnosticCategory::Error,
        message: "Invalid use of '{0}'. Modules are automatically in strict mode.",
    },
    DiagnosticMessage {
        code: 1216,
        category: DiagnosticCategory::Error,
        message: "Identifier expected. '__esModule' is reserved as an exported marker when transforming ECMAScript modules.",
    },
    DiagnosticMessage {
        code: 1218,
        category: DiagnosticCategory::Error,
        message: "Export assignment is not supported when '--module' flag is 'system'.",
    },
    DiagnosticMessage {
        code: 1221,
        category: DiagnosticCategory::Error,
        message: "Generators are not allowed in an ambient context.",
    },
    DiagnosticMessage {
        code: 1222,
        category: DiagnosticCategory::Error,
        message: "An overload signature cannot be declared as a generator.",
    },
    DiagnosticMessage {
        code: 1223,
        category: DiagnosticCategory::Error,
        message: "'{0}' tag already specified.",
    },
    DiagnosticMessage {
        code: 1224,
        category: DiagnosticCategory::Error,
        message: "Signature '{0}' must be a type predicate.",
    },
    DiagnosticMessage {
        code: 1225,
        category: DiagnosticCategory::Error,
        message: "Cannot find parameter '{0}'.",
    },
    DiagnosticMessage {
        code: 1226,
        category: DiagnosticCategory::Error,
        message: "Type predicate '{0}' is not assignable to '{1}'.",
    },
    DiagnosticMessage {
        code: 1227,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' is not in the same position as parameter '{1}'.",
    },
    DiagnosticMessage {
        code: 1228,
        category: DiagnosticCategory::Error,
        message: "A type predicate is only allowed in return type position for functions and methods.",
    },
    DiagnosticMessage {
        code: 1229,
        category: DiagnosticCategory::Error,
        message: "A type predicate cannot reference a rest parameter.",
    },
    DiagnosticMessage {
        code: 1230,
        category: DiagnosticCategory::Error,
        message: "A type predicate cannot reference element '{0}' in a binding pattern.",
    },
    DiagnosticMessage {
        code: 1231,
        category: DiagnosticCategory::Error,
        message: "An export assignment must be at the top level of a file or module declaration.",
    },
    DiagnosticMessage {
        code: 1232,
        category: DiagnosticCategory::Error,
        message: "An import declaration can only be used at the top level of a namespace or module.",
    },
    DiagnosticMessage {
        code: 1233,
        category: DiagnosticCategory::Error,
        message: "An export declaration can only be used at the top level of a namespace or module.",
    },
    DiagnosticMessage {
        code: 1234,
        category: DiagnosticCategory::Error,
        message: "An ambient module declaration is only allowed at the top level in a file.",
    },
    DiagnosticMessage {
        code: 1235,
        category: DiagnosticCategory::Error,
        message: "A namespace declaration is only allowed at the top level of a namespace or module.",
    },
    DiagnosticMessage {
        code: 1236,
        category: DiagnosticCategory::Error,
        message: "The return type of a property decorator function must be either 'void' or 'any'.",
    },
    DiagnosticMessage {
        code: 1237,
        category: DiagnosticCategory::Error,
        message: "The return type of a parameter decorator function must be either 'void' or 'any'.",
    },
    DiagnosticMessage {
        code: 1238,
        category: DiagnosticCategory::Error,
        message: "Unable to resolve signature of class decorator when called as an expression.",
    },
    DiagnosticMessage {
        code: 1239,
        category: DiagnosticCategory::Error,
        message: "Unable to resolve signature of parameter decorator when called as an expression.",
    },
    DiagnosticMessage {
        code: 1240,
        category: DiagnosticCategory::Error,
        message: "Unable to resolve signature of property decorator when called as an expression.",
    },
    DiagnosticMessage {
        code: 1241,
        category: DiagnosticCategory::Error,
        message: "Unable to resolve signature of method decorator when called as an expression.",
    },
    DiagnosticMessage {
        code: 1242,
        category: DiagnosticCategory::Error,
        message: "'abstract' modifier can only appear on a class, method, or property declaration.",
    },
    DiagnosticMessage {
        code: 1243,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot be used with '{1}' modifier.",
    },
    DiagnosticMessage {
        code: 1244,
        category: DiagnosticCategory::Error,
        message: "Abstract methods can only appear within an abstract class.",
    },
    DiagnosticMessage {
        code: 1245,
        category: DiagnosticCategory::Error,
        message: "Method '{0}' cannot have an implementation because it is marked abstract.",
    },
    DiagnosticMessage {
        code: 1246,
        category: DiagnosticCategory::Error,
        message: "An interface property cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1247,
        category: DiagnosticCategory::Error,
        message: "A type literal property cannot have an initializer.",
    },
    DiagnosticMessage {
        code: 1248,
        category: DiagnosticCategory::Error,
        message: "A class member cannot have the '{0}' keyword.",
    },
    DiagnosticMessage {
        code: 1249,
        category: DiagnosticCategory::Error,
        message: "A decorator can only decorate a method implementation, not an overload.",
    },
    DiagnosticMessage {
        code: 1250,
        category: DiagnosticCategory::Error,
        message: "Function declarations are not allowed inside blocks in strict mode when targeting 'ES5'.",
    },
    DiagnosticMessage {
        code: 1251,
        category: DiagnosticCategory::Error,
        message: "Function declarations are not allowed inside blocks in strict mode when targeting 'ES5'. Class definitions are automatically in strict mode.",
    },
    DiagnosticMessage {
        code: 1252,
        category: DiagnosticCategory::Error,
        message: "Function declarations are not allowed inside blocks in strict mode when targeting 'ES5'. Modules are automatically in strict mode.",
    },
    DiagnosticMessage {
        code: 1253,
        category: DiagnosticCategory::Error,
        message: "Abstract properties can only appear within an abstract class.",
    },
    DiagnosticMessage {
        code: 1254,
        category: DiagnosticCategory::Error,
        message: "A 'const' initializer in an ambient context must be a string or numeric literal or literal enum reference.",
    },
    DiagnosticMessage {
        code: 1255,
        category: DiagnosticCategory::Error,
        message: "A definite assignment assertion '!' is not permitted in this context.",
    },
    DiagnosticMessage {
        code: 1257,
        category: DiagnosticCategory::Error,
        message: "A required element cannot follow an optional element.",
    },
    DiagnosticMessage {
        code: 1258,
        category: DiagnosticCategory::Error,
        message: "A default export must be at the top level of a file or module declaration.",
    },
    DiagnosticMessage {
        code: 1259,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' can only be default-imported using the '{1}' flag",
    },
    DiagnosticMessage {
        code: 1260,
        category: DiagnosticCategory::Error,
        message: "Keywords cannot contain escape characters.",
    },
    DiagnosticMessage {
        code: 1261,
        category: DiagnosticCategory::Error,
        message: "Already included file name '{0}' differs from file name '{1}' only in casing.",
    },
    DiagnosticMessage {
        code: 1262,
        category: DiagnosticCategory::Error,
        message: "Identifier expected. '{0}' is a reserved word at the top-level of a module.",
    },
    DiagnosticMessage {
        code: 1263,
        category: DiagnosticCategory::Error,
        message: "Declarations with initializers cannot also have definite assignment assertions.",
    },
    DiagnosticMessage {
        code: 1264,
        category: DiagnosticCategory::Error,
        message: "Declarations with definite assignment assertions must also have type annotations.",
    },
    DiagnosticMessage {
        code: 1265,
        category: DiagnosticCategory::Error,
        message: "A rest element cannot follow another rest element.",
    },
    DiagnosticMessage {
        code: 1266,
        category: DiagnosticCategory::Error,
        message: "An optional element cannot follow a rest element.",
    },
    DiagnosticMessage {
        code: 1267,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' cannot have an initializer because it is marked abstract.",
    },
    DiagnosticMessage {
        code: 1268,
        category: DiagnosticCategory::Error,
        message: "An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.",
    },
    DiagnosticMessage {
        code: 1269,
        category: DiagnosticCategory::Error,
        message: "Cannot use 'export import' on a type or type-only namespace when '{0}' is enabled.",
    },
    DiagnosticMessage {
        code: 1270,
        category: DiagnosticCategory::Error,
        message: "Decorator function return type '{0}' is not assignable to type '{1}'.",
    },
    DiagnosticMessage {
        code: 1271,
        category: DiagnosticCategory::Error,
        message: "Decorator function return type is '{0}' but is expected to be 'void' or 'any'.",
    },
    DiagnosticMessage {
        code: 1272,
        category: DiagnosticCategory::Error,
        message: "A type referenced in a decorated signature must be imported with 'import type' or a namespace import when 'isolatedModules' and 'emitDecoratorMetadata' are enabled.",
    },
    DiagnosticMessage {
        code: 1273,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on a type parameter",
    },
    DiagnosticMessage {
        code: 1274,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier can only appear on a type parameter of a class, interface or type alias",
    },
    DiagnosticMessage {
        code: 1275,
        category: DiagnosticCategory::Error,
        message: "'accessor' modifier can only appear on a property declaration.",
    },
    DiagnosticMessage {
        code: 1276,
        category: DiagnosticCategory::Error,
        message: "An 'accessor' property cannot be declared optional.",
    },
    DiagnosticMessage {
        code: 1277,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier can only appear on a type parameter of a function, method or class",
    },
    DiagnosticMessage {
        code: 1278,
        category: DiagnosticCategory::Error,
        message: "The runtime will invoke the decorator with {1} arguments, but the decorator expects {0}.",
    },
    DiagnosticMessage {
        code: 1279,
        category: DiagnosticCategory::Error,
        message: "The runtime will invoke the decorator with {1} arguments, but the decorator expects at least {0}.",
    },
    DiagnosticMessage {
        code: 1280,
        category: DiagnosticCategory::Error,
        message: "Namespaces are not allowed in global script files when '{0}' is enabled. If this file is not intended to be a global script, set 'moduleDetection' to 'force' or add an empty 'export {}' statement.",
    },
    DiagnosticMessage {
        code: 1281,
        category: DiagnosticCategory::Error,
        message: "Cannot access '{0}' from another file without qualification when '{1}' is enabled. Use '{2}' instead.",
    },
    DiagnosticMessage {
        code: 1282,
        category: DiagnosticCategory::Error,
        message: "An 'export =' declaration must reference a value when 'verbatimModuleSyntax' is enabled, but '{0}' only refers to a type.",
    },
    DiagnosticMessage {
        code: 1283,
        category: DiagnosticCategory::Error,
        message: "An 'export =' declaration must reference a real value when 'verbatimModuleSyntax' is enabled, but '{0}' resolves to a type-only declaration.",
    },
    DiagnosticMessage {
        code: 1284,
        category: DiagnosticCategory::Error,
        message: "An 'export default' must reference a value when 'verbatimModuleSyntax' is enabled, but '{0}' only refers to a type.",
    },
    DiagnosticMessage {
        code: 1285,
        category: DiagnosticCategory::Error,
        message: "An 'export default' must reference a real value when 'verbatimModuleSyntax' is enabled, but '{0}' resolves to a type-only declaration.",
    },
    DiagnosticMessage {
        code: 1286,
        category: DiagnosticCategory::Error,
        message: "ECMAScript imports and exports cannot be written in a CommonJS file under 'verbatimModuleSyntax'.",
    },
    DiagnosticMessage {
        code: 1287,
        category: DiagnosticCategory::Error,
        message: "A top-level 'export' modifier cannot be used on value declarations in a CommonJS module when 'verbatimModuleSyntax' is enabled.",
    },
    DiagnosticMessage {
        code: 1288,
        category: DiagnosticCategory::Error,
        message: "An import alias cannot resolve to a type or type-only declaration when 'verbatimModuleSyntax' is enabled.",
    },
    DiagnosticMessage {
        code: 1289,
        category: DiagnosticCategory::Error,
        message: "'{0}' resolves to a type-only declaration and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'import type' where '{0}' is imported.",
    },
    DiagnosticMessage {
        code: 1290,
        category: DiagnosticCategory::Error,
        message: "'{0}' resolves to a type-only declaration and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'export type { {0} as default }'.",
    },
    DiagnosticMessage {
        code: 1291,
        category: DiagnosticCategory::Error,
        message: "'{0}' resolves to a type and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'import type' where '{0}' is imported.",
    },
    DiagnosticMessage {
        code: 1292,
        category: DiagnosticCategory::Error,
        message: "'{0}' resolves to a type and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'export type { {0} as default }'.",
    },
    DiagnosticMessage {
        code: 1293,
        category: DiagnosticCategory::Error,
        message: "ECMAScript module syntax is not allowed in a CommonJS module when 'module' is set to 'preserve'.",
    },
    DiagnosticMessage {
        code: 1294,
        category: DiagnosticCategory::Error,
        message: "This syntax is not allowed when 'erasableSyntaxOnly' is enabled.",
    },
    DiagnosticMessage {
        code: 1295,
        category: DiagnosticCategory::Error,
        message: "ECMAScript imports and exports cannot be written in a CommonJS file under 'verbatimModuleSyntax'. Adjust the 'type' field in the nearest 'package.json' to make this file an ECMAScript module, or adjust your 'verbatimModuleSyntax', 'module', and 'moduleResolution' settings in TypeScript.",
    },
    DiagnosticMessage {
        code: 1300,
        category: DiagnosticCategory::Error,
        message: "'with' statements are not allowed in an async function block.",
    },
    DiagnosticMessage {
        code: 1308,
        category: DiagnosticCategory::Error,
        message: "'await' expressions are only allowed within async functions and at the top levels of modules.",
    },
    DiagnosticMessage {
        code: 1309,
        category: DiagnosticCategory::Error,
        message: "The current file is a CommonJS module and cannot use 'await' at the top level.",
    },
    DiagnosticMessage {
        code: 1312,
        category: DiagnosticCategory::Error,
        message: "Did you mean to use a ':'? An '=' can only follow a property name when the containing object literal is part of a destructuring pattern.",
    },
    DiagnosticMessage {
        code: 1313,
        category: DiagnosticCategory::Error,
        message: "The body of an 'if' statement cannot be the empty statement.",
    },
    DiagnosticMessage {
        code: 1314,
        category: DiagnosticCategory::Error,
        message: "Global module exports may only appear in module files.",
    },
    DiagnosticMessage {
        code: 1315,
        category: DiagnosticCategory::Error,
        message: "Global module exports may only appear in declaration files.",
    },
    DiagnosticMessage {
        code: 1316,
        category: DiagnosticCategory::Error,
        message: "Global module exports may only appear at top level.",
    },
    DiagnosticMessage {
        code: 1317,
        category: DiagnosticCategory::Error,
        message: "A parameter property cannot be declared using a rest parameter.",
    },
    DiagnosticMessage {
        code: 1318,
        category: DiagnosticCategory::Error,
        message: "An abstract accessor cannot have an implementation.",
    },
    DiagnosticMessage {
        code: 1319,
        category: DiagnosticCategory::Error,
        message: "A default export can only be used in an ECMAScript-style module.",
    },
    DiagnosticMessage {
        code: 1320,
        category: DiagnosticCategory::Error,
        message: "Type of 'await' operand must either be a valid promise or must not contain a callable 'then' member.",
    },
    DiagnosticMessage {
        code: 1321,
        category: DiagnosticCategory::Error,
        message: "Type of 'yield' operand in an async generator must either be a valid promise or must not contain a callable 'then' member.",
    },
    DiagnosticMessage {
        code: 1322,
        category: DiagnosticCategory::Error,
        message: "Type of iterated elements of a 'yield*' operand must either be a valid promise or must not contain a callable 'then' member.",
    },
    DiagnosticMessage {
        code: 1323,
        category: DiagnosticCategory::Error,
        message: "Dynamic imports are only supported when the '--module' flag is set to 'es2020', 'es2022', 'esnext', 'commonjs', 'amd', 'system', 'umd', 'node16', 'node18', 'node20', or 'nodenext'.",
    },
    DiagnosticMessage {
        code: 1324,
        category: DiagnosticCategory::Error,
        message: "Dynamic imports only support a second argument when the '--module' option is set to 'esnext', 'node16', 'node18', 'node20', 'nodenext', or 'preserve'.",
    },
    DiagnosticMessage {
        code: 1325,
        category: DiagnosticCategory::Error,
        message: "Argument of dynamic import cannot be spread element.",
    },
    DiagnosticMessage {
        code: 1326,
        category: DiagnosticCategory::Error,
        message: "This use of 'import' is invalid. 'import()' calls can be written, but they must have parentheses and cannot have type arguments.",
    },
    DiagnosticMessage {
        code: 1327,
        category: DiagnosticCategory::Error,
        message: "String literal with double quotes expected.",
    },
    DiagnosticMessage {
        code: 1328,
        category: DiagnosticCategory::Error,
        message: "Property value can only be string literal, numeric literal, 'true', 'false', 'null', object literal or array literal.",
    },
    DiagnosticMessage {
        code: 1329,
        category: DiagnosticCategory::Error,
        message: "'{0}' accepts too few arguments to be used as a decorator here. Did you mean to call it first and write '@{0}()'?",
    },
    DiagnosticMessage {
        code: 1330,
        category: DiagnosticCategory::Error,
        message: "A property of an interface or type literal whose type is a 'unique symbol' type must be 'readonly'.",
    },
    DiagnosticMessage {
        code: 1331,
        category: DiagnosticCategory::Error,
        message: "A property of a class whose type is a 'unique symbol' type must be both 'static' and 'readonly'.",
    },
    DiagnosticMessage {
        code: 1332,
        category: DiagnosticCategory::Error,
        message: "A variable whose type is a 'unique symbol' type must be 'const'.",
    },
    DiagnosticMessage {
        code: 1333,
        category: DiagnosticCategory::Error,
        message: "'unique symbol' types may not be used on a variable declaration with a binding name.",
    },
    DiagnosticMessage {
        code: 1334,
        category: DiagnosticCategory::Error,
        message: "'unique symbol' types are only allowed on variables in a variable statement.",
    },
    DiagnosticMessage {
        code: 1335,
        category: DiagnosticCategory::Error,
        message: "'unique symbol' types are not allowed here.",
    },
    DiagnosticMessage {
        code: 1337,
        category: DiagnosticCategory::Error,
        message: "An index signature parameter type cannot be a literal type or generic type. Consider using a mapped object type instead.",
    },
    DiagnosticMessage {
        code: 1338,
        category: DiagnosticCategory::Error,
        message: "'infer' declarations are only permitted in the 'extends' clause of a conditional type.",
    },
    DiagnosticMessage {
        code: 1339,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' does not refer to a value, but is used as a value here.",
    },
    DiagnosticMessage {
        code: 1340,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' does not refer to a type, but is used as a type here. Did you mean 'typeof import('{0}')'?",
    },
    DiagnosticMessage {
        code: 1341,
        category: DiagnosticCategory::Error,
        message: "Class constructor may not be an accessor.",
    },
    DiagnosticMessage {
        code: 1343,
        category: DiagnosticCategory::Error,
        message: "The 'import.meta' meta-property is only allowed when the '--module' option is 'es2020', 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', or 'nodenext'.",
    },
    DiagnosticMessage {
        code: 1344,
        category: DiagnosticCategory::Error,
        message: "'A label is not allowed here.",
    },
    DiagnosticMessage {
        code: 1345,
        category: DiagnosticCategory::Error,
        message: "An expression of type 'void' cannot be tested for truthiness.",
    },
    DiagnosticMessage {
        code: 1346,
        category: DiagnosticCategory::Error,
        message: "This parameter is not allowed with 'use strict' directive.",
    },
    DiagnosticMessage {
        code: 1347,
        category: DiagnosticCategory::Error,
        message: "'use strict' directive cannot be used with non-simple parameter list.",
    },
    DiagnosticMessage {
        code: 1348,
        category: DiagnosticCategory::Error,
        message: "Non-simple parameter declared here.",
    },
    DiagnosticMessage {
        code: 1349,
        category: DiagnosticCategory::Error,
        message: "'use strict' directive used here.",
    },
    DiagnosticMessage {
        code: 1350,
        category: DiagnosticCategory::Message,
        message: "Print the final configuration instead of building.",
    },
    DiagnosticMessage {
        code: 1351,
        category: DiagnosticCategory::Error,
        message: "An identifier or keyword cannot immediately follow a numeric literal.",
    },
    DiagnosticMessage {
        code: 1352,
        category: DiagnosticCategory::Error,
        message: "A bigint literal cannot use exponential notation.",
    },
    DiagnosticMessage {
        code: 1353,
        category: DiagnosticCategory::Error,
        message: "A bigint literal must be an integer.",
    },
    DiagnosticMessage {
        code: 1354,
        category: DiagnosticCategory::Error,
        message: "'readonly' type modifier is only permitted on array and tuple literal types.",
    },
    DiagnosticMessage {
        code: 1355,
        category: DiagnosticCategory::Error,
        message: "A 'const' assertion can only be applied to references to enum members, or string, number, boolean, array, or object literals.",
    },
    DiagnosticMessage {
        code: 1356,
        category: DiagnosticCategory::Error,
        message: "Did you mean to mark this function as 'async'?",
    },
    DiagnosticMessage {
        code: 1357,
        category: DiagnosticCategory::Error,
        message: "An enum member name must be followed by a ',', '=', or '}'.",
    },
    DiagnosticMessage {
        code: 1358,
        category: DiagnosticCategory::Error,
        message: "Tagged template expressions are not permitted in an optional chain.",
    },
    DiagnosticMessage {
        code: 1359,
        category: DiagnosticCategory::Error,
        message: "Identifier expected. '{0}' is a reserved word that cannot be used here.",
    },
    DiagnosticMessage {
        code: 1360,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' does not satisfy the expected type '{1}'.",
    },
    DiagnosticMessage {
        code: 1361,
        category: DiagnosticCategory::Error,
        message: "'{0}' cannot be used as a value because it was imported using 'import type'.",
    },
    DiagnosticMessage {
        code: 1362,
        category: DiagnosticCategory::Error,
        message: "'{0}' cannot be used as a value because it was exported using 'export type'.",
    },
    DiagnosticMessage {
        code: 1363,
        category: DiagnosticCategory::Error,
        message: "A type-only import can specify a default import or named bindings, but not both.",
    },
    DiagnosticMessage {
        code: 1364,
        category: DiagnosticCategory::Message,
        message: "Convert to type-only export",
    },
    DiagnosticMessage {
        code: 1365,
        category: DiagnosticCategory::Message,
        message: "Convert all re-exported types to type-only exports",
    },
    DiagnosticMessage {
        code: 1366,
        category: DiagnosticCategory::Message,
        message: "Split into two separate import declarations",
    },
    DiagnosticMessage {
        code: 1367,
        category: DiagnosticCategory::Message,
        message: "Split all invalid type-only imports",
    },
    DiagnosticMessage {
        code: 1368,
        category: DiagnosticCategory::Error,
        message: "Class constructor may not be a generator.",
    },
    DiagnosticMessage {
        code: 1369,
        category: DiagnosticCategory::Message,
        message: "Did you mean '{0}'?",
    },
    DiagnosticMessage {
        code: 1375,
        category: DiagnosticCategory::Error,
        message: "'await' expressions are only allowed at the top level of a file when that file is a module, but this file has no imports or exports. Consider adding an empty 'export {}' to make this file a module.",
    },
    DiagnosticMessage {
        code: 1376,
        category: DiagnosticCategory::Message,
        message: "'{0}' was imported here.",
    },
    DiagnosticMessage {
        code: 1377,
        category: DiagnosticCategory::Message,
        message: "'{0}' was exported here.",
    },
    DiagnosticMessage {
        code: 1378,
        category: DiagnosticCategory::Error,
        message: "Top-level 'await' expressions are only allowed when the 'module' option is set to 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', 'nodenext', or 'preserve', and the 'target' option is set to 'es2017' or higher.",
    },
    DiagnosticMessage {
        code: 1379,
        category: DiagnosticCategory::Error,
        message: "An import alias cannot reference a declaration that was exported using 'export type'.",
    },
    DiagnosticMessage {
        code: 1380,
        category: DiagnosticCategory::Error,
        message: "An import alias cannot reference a declaration that was imported using 'import type'.",
    },
    DiagnosticMessage {
        code: 1381,
        category: DiagnosticCategory::Error,
        message: "Unexpected token. Did you mean `{'}'}` or `&rbrace;`?",
    },
    DiagnosticMessage {
        code: 1382,
        category: DiagnosticCategory::Error,
        message: "Unexpected token. Did you mean `{'>'}` or `&gt;`?",
    },
    DiagnosticMessage {
        code: 1385,
        category: DiagnosticCategory::Error,
        message: "Function type notation must be parenthesized when used in a union type.",
    },
    DiagnosticMessage {
        code: 1386,
        category: DiagnosticCategory::Error,
        message: "Constructor type notation must be parenthesized when used in a union type.",
    },
    DiagnosticMessage {
        code: 1387,
        category: DiagnosticCategory::Error,
        message: "Function type notation must be parenthesized when used in an intersection type.",
    },
    DiagnosticMessage {
        code: 1388,
        category: DiagnosticCategory::Error,
        message: "Constructor type notation must be parenthesized when used in an intersection type.",
    },
    DiagnosticMessage {
        code: 1389,
        category: DiagnosticCategory::Error,
        message: "'{0}' is not allowed as a variable declaration name.",
    },
    DiagnosticMessage {
        code: 1390,
        category: DiagnosticCategory::Error,
        message: "'{0}' is not allowed as a parameter name.",
    },
    DiagnosticMessage {
        code: 1392,
        category: DiagnosticCategory::Error,
        message: "An import alias cannot use 'import type'",
    },
    DiagnosticMessage {
        code: 1393,
        category: DiagnosticCategory::Message,
        message: "Imported via {0} from file '{1}'",
    },
    DiagnosticMessage {
        code: 1394,
        category: DiagnosticCategory::Message,
        message: "Imported via {0} from file '{1}' with packageId '{2}'",
    },
    DiagnosticMessage {
        code: 1395,
        category: DiagnosticCategory::Message,
        message: "Imported via {0} from file '{1}' to import 'importHelpers' as specified in compilerOptions",
    },
    DiagnosticMessage {
        code: 1396,
        category: DiagnosticCategory::Message,
        message: "Imported via {0} from file '{1}' with packageId '{2}' to import 'importHelpers' as specified in compilerOptions",
    },
    DiagnosticMessage {
        code: 1397,
        category: DiagnosticCategory::Message,
        message: "Imported via {0} from file '{1}' to import 'jsx' and 'jsxs' factory functions",
    },
    DiagnosticMessage {
        code: 1398,
        category: DiagnosticCategory::Message,
        message: "Imported via {0} from file '{1}' with packageId '{2}' to import 'jsx' and 'jsxs' factory functions",
    },
    DiagnosticMessage {
        code: 1399,
        category: DiagnosticCategory::Message,
        message: "File is included via import here.",
    },
    DiagnosticMessage {
        code: 1400,
        category: DiagnosticCategory::Message,
        message: "Referenced via '{0}' from file '{1}'",
    },
    DiagnosticMessage {
        code: 1401,
        category: DiagnosticCategory::Message,
        message: "File is included via reference here.",
    },
    DiagnosticMessage {
        code: 1402,
        category: DiagnosticCategory::Message,
        message: "Type library referenced via '{0}' from file '{1}'",
    },
    DiagnosticMessage {
        code: 1403,
        category: DiagnosticCategory::Message,
        message: "Type library referenced via '{0}' from file '{1}' with packageId '{2}'",
    },
    DiagnosticMessage {
        code: 1404,
        category: DiagnosticCategory::Message,
        message: "File is included via type library reference here.",
    },
    DiagnosticMessage {
        code: 1405,
        category: DiagnosticCategory::Message,
        message: "Library referenced via '{0}' from file '{1}'",
    },
    DiagnosticMessage {
        code: 1406,
        category: DiagnosticCategory::Message,
        message: "File is included via library reference here.",
    },
    DiagnosticMessage {
        code: 1407,
        category: DiagnosticCategory::Message,
        message: "Matched by include pattern '{0}' in '{1}'",
    },
    DiagnosticMessage {
        code: 1408,
        category: DiagnosticCategory::Message,
        message: "File is matched by include pattern specified here.",
    },
    DiagnosticMessage {
        code: 1409,
        category: DiagnosticCategory::Message,
        message: "Part of 'files' list in tsconfig.json",
    },
    DiagnosticMessage {
        code: 1410,
        category: DiagnosticCategory::Message,
        message: "File is matched by 'files' list specified here.",
    },
    DiagnosticMessage {
        code: 1411,
        category: DiagnosticCategory::Message,
        message: "Output from referenced project '{0}' included because '{1}' specified",
    },
    DiagnosticMessage {
        code: 1412,
        category: DiagnosticCategory::Message,
        message: "Output from referenced project '{0}' included because '--module' is specified as 'none'",
    },
    DiagnosticMessage {
        code: 1413,
        category: DiagnosticCategory::Message,
        message: "File is output from referenced project specified here.",
    },
    DiagnosticMessage {
        code: 1414,
        category: DiagnosticCategory::Message,
        message: "Source from referenced project '{0}' included because '{1}' specified",
    },
    DiagnosticMessage {
        code: 1415,
        category: DiagnosticCategory::Message,
        message: "Source from referenced project '{0}' included because '--module' is specified as 'none'",
    },
    DiagnosticMessage {
        code: 1416,
        category: DiagnosticCategory::Message,
        message: "File is source from referenced project specified here.",
    },
    DiagnosticMessage {
        code: 1417,
        category: DiagnosticCategory::Message,
        message: "Entry point of type library '{0}' specified in compilerOptions",
    },
    DiagnosticMessage {
        code: 1418,
        category: DiagnosticCategory::Message,
        message: "Entry point of type library '{0}' specified in compilerOptions with packageId '{1}'",
    },
    DiagnosticMessage {
        code: 1419,
        category: DiagnosticCategory::Message,
        message: "File is entry point of type library specified here.",
    },
    DiagnosticMessage {
        code: 1420,
        category: DiagnosticCategory::Message,
        message: "Entry point for implicit type library '{0}'",
    },
    DiagnosticMessage {
        code: 1421,
        category: DiagnosticCategory::Message,
        message: "Entry point for implicit type library '{0}' with packageId '{1}'",
    },
    DiagnosticMessage {
        code: 1422,
        category: DiagnosticCategory::Message,
        message: "Library '{0}' specified in compilerOptions",
    },
    DiagnosticMessage {
        code: 1423,
        category: DiagnosticCategory::Message,
        message: "File is library specified here.",
    },
    DiagnosticMessage {
        code: 1424,
        category: DiagnosticCategory::Message,
        message: "Default library",
    },
    DiagnosticMessage {
        code: 1425,
        category: DiagnosticCategory::Message,
        message: "Default library for target '{0}'",
    },
    DiagnosticMessage {
        code: 1426,
        category: DiagnosticCategory::Message,
        message: "File is default library for target specified here.",
    },
    DiagnosticMessage {
        code: 1427,
        category: DiagnosticCategory::Message,
        message: "Root file specified for compilation",
    },
    DiagnosticMessage {
        code: 1428,
        category: DiagnosticCategory::Message,
        message: "File is output of project reference source '{0}'",
    },
    DiagnosticMessage {
        code: 1429,
        category: DiagnosticCategory::Message,
        message: "File redirects to file '{0}'",
    },
    DiagnosticMessage {
        code: 1430,
        category: DiagnosticCategory::Message,
        message: "The file is in the program because:",
    },
    DiagnosticMessage {
        code: 1431,
        category: DiagnosticCategory::Error,
        message: "'for await' loops are only allowed at the top level of a file when that file is a module, but this file has no imports or exports. Consider adding an empty 'export {}' to make this file a module.",
    },
    DiagnosticMessage {
        code: 1432,
        category: DiagnosticCategory::Error,
        message: "Top-level 'for await' loops are only allowed when the 'module' option is set to 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', 'nodenext', or 'preserve', and the 'target' option is set to 'es2017' or higher.",
    },
    DiagnosticMessage {
        code: 1433,
        category: DiagnosticCategory::Error,
        message: "Neither decorators nor modifiers may be applied to 'this' parameters.",
    },
    DiagnosticMessage {
        code: 1434,
        category: DiagnosticCategory::Error,
        message: "Unexpected keyword or identifier.",
    },
    DiagnosticMessage {
        code: 1435,
        category: DiagnosticCategory::Error,
        message: "Unknown keyword or identifier. Did you mean '{0}'?",
    },
    DiagnosticMessage {
        code: 1436,
        category: DiagnosticCategory::Error,
        message: "Decorators must precede the name and all keywords of property declarations.",
    },
    DiagnosticMessage {
        code: 1437,
        category: DiagnosticCategory::Error,
        message: "Namespace must be given a name.",
    },
    DiagnosticMessage {
        code: 1438,
        category: DiagnosticCategory::Error,
        message: "Interface must be given a name.",
    },
    DiagnosticMessage {
        code: 1439,
        category: DiagnosticCategory::Error,
        message: "Type alias must be given a name.",
    },
    DiagnosticMessage {
        code: 1440,
        category: DiagnosticCategory::Error,
        message: "Variable declaration not allowed at this location.",
    },
    DiagnosticMessage {
        code: 1441,
        category: DiagnosticCategory::Error,
        message: "Cannot start a function call in a type annotation.",
    },
    DiagnosticMessage {
        code: 1442,
        category: DiagnosticCategory::Error,
        message: "Expected '=' for property initializer.",
    },
    DiagnosticMessage {
        code: 1443,
        category: DiagnosticCategory::Error,
        message: "Module declaration names may only use ' or \" quoted strings.",
    },
    DiagnosticMessage {
        code: 1448,
        category: DiagnosticCategory::Error,
        message: "'{0}' resolves to a type-only declaration and must be re-exported using a type-only re-export when '{1}' is enabled.",
    },
    DiagnosticMessage {
        code: 1449,
        category: DiagnosticCategory::Message,
        message: "Preserve unused imported values in the JavaScript output that would otherwise be removed.",
    },
    DiagnosticMessage {
        code: 1450,
        category: DiagnosticCategory::Message,
        message: "Dynamic imports can only accept a module specifier and an optional set of attributes as arguments",
    },
    DiagnosticMessage {
        code: 1451,
        category: DiagnosticCategory::Error,
        message: "Private identifiers are only allowed in class bodies and may only be used as part of a class member declaration, property access, or on the left-hand-side of an 'in' expression",
    },
    DiagnosticMessage {
        code: 1453,
        category: DiagnosticCategory::Error,
        message: "`resolution-mode` should be either `require` or `import`.",
    },
    DiagnosticMessage {
        code: 1454,
        category: DiagnosticCategory::Error,
        message: "`resolution-mode` can only be set for type-only imports.",
    },
    DiagnosticMessage {
        code: 1455,
        category: DiagnosticCategory::Error,
        message: "`resolution-mode` is the only valid key for type import assertions.",
    },
    DiagnosticMessage {
        code: 1456,
        category: DiagnosticCategory::Error,
        message: "Type import assertions should have exactly one key - `resolution-mode` - with value `import` or `require`.",
    },
    DiagnosticMessage {
        code: 1457,
        category: DiagnosticCategory::Message,
        message: "Matched by default include pattern '**/*'",
    },
    DiagnosticMessage {
        code: 1458,
        category: DiagnosticCategory::Message,
        message: "File is ECMAScript module because '{0}' has field \"type\" with value \"module\"",
    },
    DiagnosticMessage {
        code: 1459,
        category: DiagnosticCategory::Message,
        message: "File is CommonJS module because '{0}' has field \"type\" whose value is not \"module\"",
    },
    DiagnosticMessage {
        code: 1460,
        category: DiagnosticCategory::Message,
        message: "File is CommonJS module because '{0}' does not have field \"type\"",
    },
    DiagnosticMessage {
        code: 1461,
        category: DiagnosticCategory::Message,
        message: "File is CommonJS module because 'package.json' was not found",
    },
    DiagnosticMessage {
        code: 1463,
        category: DiagnosticCategory::Error,
        message: "'resolution-mode' is the only valid key for type import attributes.",
    },
    DiagnosticMessage {
        code: 1464,
        category: DiagnosticCategory::Error,
        message: "Type import attributes should have exactly one key - 'resolution-mode' - with value 'import' or 'require'.",
    },
    DiagnosticMessage {
        code: 1470,
        category: DiagnosticCategory::Error,
        message: "The 'import.meta' meta-property is not allowed in files which will build into CommonJS output.",
    },
    DiagnosticMessage {
        code: 1471,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' cannot be imported using this construct. The specifier only resolves to an ES module, which cannot be imported with 'require'. Use an ECMAScript import instead.",
    },
    DiagnosticMessage {
        code: 1472,
        category: DiagnosticCategory::Error,
        message: "'catch' or 'finally' expected.",
    },
    DiagnosticMessage {
        code: 1473,
        category: DiagnosticCategory::Error,
        message: "An import declaration can only be used at the top level of a module.",
    },
    DiagnosticMessage {
        code: 1474,
        category: DiagnosticCategory::Error,
        message: "An export declaration can only be used at the top level of a module.",
    },
    DiagnosticMessage {
        code: 1475,
        category: DiagnosticCategory::Message,
        message: "Control what method is used to detect module-format JS files.",
    },
    DiagnosticMessage {
        code: 1476,
        category: DiagnosticCategory::Message,
        message: "\"auto\": Treat files with imports, exports, import.meta, jsx (with jsx: react-jsx), or esm format (with module: node16+) as modules.",
    },
    DiagnosticMessage {
        code: 1477,
        category: DiagnosticCategory::Error,
        message: "An instantiation expression cannot be followed by a property access.",
    },
    DiagnosticMessage {
        code: 1478,
        category: DiagnosticCategory::Error,
        message: "Identifier or string literal expected.",
    },
    DiagnosticMessage {
        code: 1479,
        category: DiagnosticCategory::Error,
        message: "The current file is a CommonJS module whose imports will produce 'require' calls; however, the referenced file is an ECMAScript module and cannot be imported with 'require'. Consider writing a dynamic 'import(\"{0}\")' call instead.",
    },
    DiagnosticMessage {
        code: 1480,
        category: DiagnosticCategory::Message,
        message: "To convert this file to an ECMAScript module, change its file extension to '{0}' or create a local package.json file with `{ \"type\": \"module\" }`.",
    },
    DiagnosticMessage {
        code: 1481,
        category: DiagnosticCategory::Message,
        message: "To convert this file to an ECMAScript module, change its file extension to '{0}', or add the field `\"type\": \"module\"` to '{1}'.",
    },
    DiagnosticMessage {
        code: 1482,
        category: DiagnosticCategory::Message,
        message: "To convert this file to an ECMAScript module, add the field `\"type\": \"module\"` to '{0}'.",
    },
    DiagnosticMessage {
        code: 1483,
        category: DiagnosticCategory::Message,
        message: "To convert this file to an ECMAScript module, create a local package.json file with `{ \"type\": \"module\" }`.",
    },
    DiagnosticMessage {
        code: 1484,
        category: DiagnosticCategory::Error,
        message: "'{0}' is a type and must be imported using a type-only import when 'verbatimModuleSyntax' is enabled.",
    },
    DiagnosticMessage {
        code: 1485,
        category: DiagnosticCategory::Error,
        message: "'{0}' resolves to a type-only declaration and must be imported using a type-only import when 'verbatimModuleSyntax' is enabled.",
    },
    DiagnosticMessage {
        code: 1486,
        category: DiagnosticCategory::Error,
        message: "Decorator used before 'export' here.",
    },
    DiagnosticMessage {
        code: 1487,
        category: DiagnosticCategory::Error,
        message: "Octal escape sequences are not allowed. Use the syntax '{0}'.",
    },
    DiagnosticMessage {
        code: 1488,
        category: DiagnosticCategory::Error,
        message: "Escape sequence '{0}' is not allowed.",
    },
    DiagnosticMessage {
        code: 1489,
        category: DiagnosticCategory::Error,
        message: "Decimals with leading zeros are not allowed.",
    },
    DiagnosticMessage {
        code: 1490,
        category: DiagnosticCategory::Error,
        message: "File appears to be binary.",
    },
    DiagnosticMessage {
        code: 1491,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on a 'using' declaration.",
    },
    DiagnosticMessage {
        code: 1492,
        category: DiagnosticCategory::Error,
        message: "'{0}' declarations may not have binding patterns.",
    },
    DiagnosticMessage {
        code: 1493,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement cannot be a 'using' declaration.",
    },
    DiagnosticMessage {
        code: 1494,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement cannot be an 'await using' declaration.",
    },
    DiagnosticMessage {
        code: 1495,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot appear on an 'await using' declaration.",
    },
    DiagnosticMessage {
        code: 1496,
        category: DiagnosticCategory::Error,
        message: "Identifier, string literal, or number literal expected.",
    },
    DiagnosticMessage {
        code: 1497,
        category: DiagnosticCategory::Error,
        message: "Expression must be enclosed in parentheses to be used as a decorator.",
    },
    DiagnosticMessage {
        code: 1498,
        category: DiagnosticCategory::Error,
        message: "Invalid syntax in decorator.",
    },
    DiagnosticMessage {
        code: 1499,
        category: DiagnosticCategory::Error,
        message: "Unknown regular expression flag.",
    },
    DiagnosticMessage {
        code: 1500,
        category: DiagnosticCategory::Error,
        message: "Duplicate regular expression flag.",
    },
    DiagnosticMessage {
        code: 1501,
        category: DiagnosticCategory::Error,
        message: "This regular expression flag is only available when targeting '{0}' or later.",
    },
    DiagnosticMessage {
        code: 1502,
        category: DiagnosticCategory::Error,
        message: "The Unicode (u) flag and the Unicode Sets (v) flag cannot be set simultaneously.",
    },
    DiagnosticMessage {
        code: 1503,
        category: DiagnosticCategory::Error,
        message: "Named capturing groups are only available when targeting 'ES2018' or later.",
    },
    DiagnosticMessage {
        code: 1504,
        category: DiagnosticCategory::Error,
        message: "Subpattern flags must be present when there is a minus sign.",
    },
    DiagnosticMessage {
        code: 1505,
        category: DiagnosticCategory::Error,
        message: "Incomplete quantifier. Digit expected.",
    },
    DiagnosticMessage {
        code: 1506,
        category: DiagnosticCategory::Error,
        message: "Numbers out of order in quantifier.",
    },
    DiagnosticMessage {
        code: 1507,
        category: DiagnosticCategory::Error,
        message: "There is nothing available for repetition.",
    },
    DiagnosticMessage {
        code: 1508,
        category: DiagnosticCategory::Error,
        message: "Unexpected '{0}'. Did you mean to escape it with backslash?",
    },
    DiagnosticMessage {
        code: 1509,
        category: DiagnosticCategory::Error,
        message: "This regular expression flag cannot be toggled within a subpattern.",
    },
    DiagnosticMessage {
        code: 1510,
        category: DiagnosticCategory::Error,
        message: "'\\k' must be followed by a capturing group name enclosed in angle brackets.",
    },
    DiagnosticMessage {
        code: 1511,
        category: DiagnosticCategory::Error,
        message: "'\\q' is only available inside character class.",
    },
    DiagnosticMessage {
        code: 1512,
        category: DiagnosticCategory::Error,
        message: "'\\c' must be followed by an ASCII letter.",
    },
    DiagnosticMessage {
        code: 1513,
        category: DiagnosticCategory::Error,
        message: "Undetermined character escape.",
    },
    DiagnosticMessage {
        code: 1514,
        category: DiagnosticCategory::Error,
        message: "Expected a capturing group name.",
    },
    DiagnosticMessage {
        code: 1515,
        category: DiagnosticCategory::Error,
        message: "Named capturing groups with the same name must be mutually exclusive to each other.",
    },
    DiagnosticMessage {
        code: 1516,
        category: DiagnosticCategory::Error,
        message: "A character class range must not be bounded by another character class.",
    },
    DiagnosticMessage {
        code: 1517,
        category: DiagnosticCategory::Error,
        message: "Range out of order in character class.",
    },
    DiagnosticMessage {
        code: 1518,
        category: DiagnosticCategory::Error,
        message: "Anything that would possibly match more than a single character is invalid inside a negated character class.",
    },
    DiagnosticMessage {
        code: 1519,
        category: DiagnosticCategory::Error,
        message: "Operators must not be mixed within a character class. Wrap it in a nested class instead.",
    },
    DiagnosticMessage {
        code: 1520,
        category: DiagnosticCategory::Error,
        message: "Expected a class set operand.",
    },
    DiagnosticMessage {
        code: 1521,
        category: DiagnosticCategory::Error,
        message: "'\\q' must be followed by string alternatives enclosed in braces.",
    },
    DiagnosticMessage {
        code: 1522,
        category: DiagnosticCategory::Error,
        message: "A character class must not contain a reserved double punctuator. Did you mean to escape it with backslash?",
    },
    DiagnosticMessage {
        code: 1523,
        category: DiagnosticCategory::Error,
        message: "Expected a Unicode property name.",
    },
    DiagnosticMessage {
        code: 1524,
        category: DiagnosticCategory::Error,
        message: "Unknown Unicode property name.",
    },
    DiagnosticMessage {
        code: 1525,
        category: DiagnosticCategory::Error,
        message: "Expected a Unicode property value.",
    },
    DiagnosticMessage {
        code: 1526,
        category: DiagnosticCategory::Error,
        message: "Unknown Unicode property value.",
    },
    DiagnosticMessage {
        code: 1527,
        category: DiagnosticCategory::Error,
        message: "Expected a Unicode property name or value.",
    },
    DiagnosticMessage {
        code: 1528,
        category: DiagnosticCategory::Error,
        message: "Any Unicode property that would possibly match more than a single character is only available when the Unicode Sets (v) flag is set.",
    },
    DiagnosticMessage {
        code: 1529,
        category: DiagnosticCategory::Error,
        message: "Unknown Unicode property name or value.",
    },
    DiagnosticMessage {
        code: 1530,
        category: DiagnosticCategory::Error,
        message: "Unicode property value expressions are only available when the Unicode (u) flag or the Unicode Sets (v) flag is set.",
    },
    DiagnosticMessage {
        code: 1531,
        category: DiagnosticCategory::Error,
        message: "'\\{0}' must be followed by a Unicode property value expression enclosed in braces.",
    },
    DiagnosticMessage {
        code: 1532,
        category: DiagnosticCategory::Error,
        message: "There is no capturing group named '{0}' in this regular expression.",
    },
    DiagnosticMessage {
        code: 1533,
        category: DiagnosticCategory::Error,
        message: "This backreference refers to a group that does not exist. There are only {0} capturing groups in this regular expression.",
    },
    DiagnosticMessage {
        code: 1534,
        category: DiagnosticCategory::Error,
        message: "This backreference refers to a group that does not exist. There are no capturing groups in this regular expression.",
    },
    DiagnosticMessage {
        code: 1535,
        category: DiagnosticCategory::Error,
        message: "This character cannot be escaped in a regular expression.",
    },
    DiagnosticMessage {
        code: 1536,
        category: DiagnosticCategory::Error,
        message: "Octal escape sequences and backreferences are not allowed in a character class. If this was intended as an escape sequence, use the syntax '{0}' instead.",
    },
    DiagnosticMessage {
        code: 1537,
        category: DiagnosticCategory::Error,
        message: "Decimal escape sequences and backreferences are not allowed in a character class.",
    },
    DiagnosticMessage {
        code: 1538,
        category: DiagnosticCategory::Error,
        message: "Unicode escape sequences are only available when the Unicode (u) flag or the Unicode Sets (v) flag is set.",
    },
    DiagnosticMessage {
        code: 1539,
        category: DiagnosticCategory::Error,
        message: "A 'bigint' literal cannot be used as a property name.",
    },
    DiagnosticMessage {
        code: 1540,
        category: DiagnosticCategory::Error,
        message: "A 'namespace' declaration should not be declared using the 'module' keyword. Please use the 'namespace' keyword instead.",
    },
    DiagnosticMessage {
        code: 1541,
        category: DiagnosticCategory::Error,
        message: "Type-only import of an ECMAScript module from a CommonJS module must have a 'resolution-mode' attribute.",
    },
    DiagnosticMessage {
        code: 1542,
        category: DiagnosticCategory::Error,
        message: "Type import of an ECMAScript module from a CommonJS module must have a 'resolution-mode' attribute.",
    },
    DiagnosticMessage {
        code: 1543,
        category: DiagnosticCategory::Error,
        message: "Importing a JSON file into an ECMAScript module requires a 'type: \"json\"' import attribute when 'module' is set to '{0}'.",
    },
    DiagnosticMessage {
        code: 1544,
        category: DiagnosticCategory::Error,
        message: "Named imports from a JSON file into an ECMAScript module are not allowed when 'module' is set to '{0}'.",
    },
    DiagnosticMessage {
        code: 1545,
        category: DiagnosticCategory::Error,
        message: "'using' declarations are not allowed in ambient contexts.",
    },
    DiagnosticMessage {
        code: 1546,
        category: DiagnosticCategory::Error,
        message: "'await using' declarations are not allowed in ambient contexts.",
    },
    DiagnosticMessage {
        code: 1547,
        category: DiagnosticCategory::Error,
        message: "'using' declarations are not allowed in 'case' or 'default' clauses unless contained within a block.",
    },
    DiagnosticMessage {
        code: 1548,
        category: DiagnosticCategory::Error,
        message: "'await using' declarations are not allowed in 'case' or 'default' clauses unless contained within a block.",
    },
    DiagnosticMessage {
        code: 1549,
        category: DiagnosticCategory::Message,
        message: "Ignore the tsconfig found and build with commandline options and files.",
    },
    DiagnosticMessage {
        code: 2200,
        category: DiagnosticCategory::Error,
        message: "The types of '{0}' are incompatible between these types.",
    },
    DiagnosticMessage {
        code: 2201,
        category: DiagnosticCategory::Error,
        message: "The types returned by '{0}' are incompatible between these types.",
    },
    DiagnosticMessage {
        code: 2202,
        category: DiagnosticCategory::Error,
        message: "Call signature return types '{0}' and '{1}' are incompatible.",
    },
    DiagnosticMessage {
        code: 2203,
        category: DiagnosticCategory::Error,
        message: "Construct signature return types '{0}' and '{1}' are incompatible.",
    },
    DiagnosticMessage {
        code: 2204,
        category: DiagnosticCategory::Error,
        message: "Call signatures with no arguments have incompatible return types '{0}' and '{1}'.",
    },
    DiagnosticMessage {
        code: 2205,
        category: DiagnosticCategory::Error,
        message: "Construct signatures with no arguments have incompatible return types '{0}' and '{1}'.",
    },
    DiagnosticMessage {
        code: 2206,
        category: DiagnosticCategory::Error,
        message: "The 'type' modifier cannot be used on a named import when 'import type' is used on its import statement.",
    },
    DiagnosticMessage {
        code: 2207,
        category: DiagnosticCategory::Error,
        message: "The 'type' modifier cannot be used on a named export when 'export type' is used on its export statement.",
    },
    DiagnosticMessage {
        code: 2208,
        category: DiagnosticCategory::Error,
        message: "This type parameter might need an `extends {0}` constraint.",
    },
    DiagnosticMessage {
        code: 2209,
        category: DiagnosticCategory::Error,
        message: "The project root is ambiguous, but is required to resolve export map entry '{0}' in file '{1}'. Supply the `rootDir` compiler option to disambiguate.",
    },
    DiagnosticMessage {
        code: 2210,
        category: DiagnosticCategory::Error,
        message: "The project root is ambiguous, but is required to resolve import map entry '{0}' in file '{1}'. Supply the `rootDir` compiler option to disambiguate.",
    },
    DiagnosticMessage {
        code: 2211,
        category: DiagnosticCategory::Message,
        message: "Add `extends` constraint.",
    },
    DiagnosticMessage {
        code: 2212,
        category: DiagnosticCategory::Message,
        message: "Add `extends` constraint to all type parameters",
    },
    DiagnosticMessage {
        code: 2300,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'.",
    },
    DiagnosticMessage {
        code: 2301,
        category: DiagnosticCategory::Error,
        message: "Initializer of instance member variable '{0}' cannot reference identifier '{1}' declared in the constructor.",
    },
    DiagnosticMessage {
        code: 2302,
        category: DiagnosticCategory::Error,
        message: "Static members cannot reference class type parameters.",
    },
    DiagnosticMessage {
        code: 2303,
        category: DiagnosticCategory::Error,
        message: "Circular definition of import alias '{0}'.",
    },
    DiagnosticMessage {
        code: 2304,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'.",
    },
    DiagnosticMessage {
        code: 2305,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' has no exported member '{1}'.",
    },
    DiagnosticMessage {
        code: 2306,
        category: DiagnosticCategory::Error,
        message: "File '{0}' is not a module.",
    },
    DiagnosticMessage {
        code: 2307,
        category: DiagnosticCategory::Error,
        message: "Cannot find module '{0}' or its corresponding type declarations.",
    },
    DiagnosticMessage {
        code: 2308,
        category: DiagnosticCategory::Error,
        message: "Module {0} has already exported a member named '{1}'. Consider explicitly re-exporting to resolve the ambiguity.",
    },
    DiagnosticMessage {
        code: 2309,
        category: DiagnosticCategory::Error,
        message: "An export assignment cannot be used in a module with other exported elements.",
    },
    DiagnosticMessage {
        code: 2310,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' recursively references itself as a base type.",
    },
    DiagnosticMessage {
        code: 2311,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Did you mean to write this in an async function?",
    },
    DiagnosticMessage {
        code: 2312,
        category: DiagnosticCategory::Error,
        message: "An interface can only extend an object type or intersection of object types with statically known members.",
    },
    DiagnosticMessage {
        code: 2313,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' has a circular constraint.",
    },
    DiagnosticMessage {
        code: 2314,
        category: DiagnosticCategory::Error,
        message: "Generic type '{0}' requires {1} type argument(s).",
    },
    DiagnosticMessage {
        code: 2315,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not generic.",
    },
    DiagnosticMessage {
        code: 2316,
        category: DiagnosticCategory::Error,
        message: "Global type '{0}' must be a class or interface type.",
    },
    DiagnosticMessage {
        code: 2317,
        category: DiagnosticCategory::Error,
        message: "Global type '{0}' must have {1} type parameter(s).",
    },
    DiagnosticMessage {
        code: 2318,
        category: DiagnosticCategory::Error,
        message: "Cannot find global type '{0}'.",
    },
    DiagnosticMessage {
        code: 2319,
        category: DiagnosticCategory::Error,
        message: "Named property '{0}' of types '{1}' and '{2}' are not identical.",
    },
    DiagnosticMessage {
        code: 2320,
        category: DiagnosticCategory::Error,
        message: "Interface '{0}' cannot simultaneously extend types '{1}' and '{2}'.",
    },
    DiagnosticMessage {
        code: 2321,
        category: DiagnosticCategory::Error,
        message: "Excessive stack depth comparing types '{0}' and '{1}'.",
    },
    DiagnosticMessage {
        code: 2322,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}'.",
    },
    DiagnosticMessage {
        code: 2323,
        category: DiagnosticCategory::Error,
        message: "Cannot redeclare exported variable '{0}'.",
    },
    DiagnosticMessage {
        code: 2324,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is missing in type '{1}'.",
    },
    DiagnosticMessage {
        code: 2325,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is private in type '{1}' but not in type '{2}'.",
    },
    DiagnosticMessage {
        code: 2326,
        category: DiagnosticCategory::Error,
        message: "Types of property '{0}' are incompatible.",
    },
    DiagnosticMessage {
        code: 2327,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is optional in type '{1}' but required in type '{2}'.",
    },
    DiagnosticMessage {
        code: 2328,
        category: DiagnosticCategory::Error,
        message: "Types of parameters '{0}' and '{1}' are incompatible.",
    },
    DiagnosticMessage {
        code: 2329,
        category: DiagnosticCategory::Error,
        message: "Index signature for type '{0}' is missing in type '{1}'.",
    },
    DiagnosticMessage {
        code: 2330,
        category: DiagnosticCategory::Error,
        message: "'{0}' and '{1}' index signatures are incompatible.",
    },
    DiagnosticMessage {
        code: 2331,
        category: DiagnosticCategory::Error,
        message: "'this' cannot be referenced in a module or namespace body.",
    },
    DiagnosticMessage {
        code: 2332,
        category: DiagnosticCategory::Error,
        message: "'this' cannot be referenced in current location.",
    },
    DiagnosticMessage {
        code: 2334,
        category: DiagnosticCategory::Error,
        message: "'this' cannot be referenced in a static property initializer.",
    },
    DiagnosticMessage {
        code: 2335,
        category: DiagnosticCategory::Error,
        message: "'super' can only be referenced in a derived class.",
    },
    DiagnosticMessage {
        code: 2336,
        category: DiagnosticCategory::Error,
        message: "'super' cannot be referenced in constructor arguments.",
    },
    DiagnosticMessage {
        code: 2337,
        category: DiagnosticCategory::Error,
        message: "Super calls are not permitted outside constructors or in nested functions inside constructors.",
    },
    DiagnosticMessage {
        code: 2338,
        category: DiagnosticCategory::Error,
        message: "'super' property access is permitted only in a constructor, member function, or member accessor of a derived class.",
    },
    DiagnosticMessage {
        code: 2339,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'.",
    },
    DiagnosticMessage {
        code: 2340,
        category: DiagnosticCategory::Error,
        message: "Only public and protected methods of the base class are accessible via the 'super' keyword.",
    },
    DiagnosticMessage {
        code: 2341,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is private and only accessible within class '{1}'.",
    },
    DiagnosticMessage {
        code: 2343,
        category: DiagnosticCategory::Error,
        message: "This syntax requires an imported helper named '{1}' which does not exist in '{0}'. Consider upgrading your version of '{0}'.",
    },
    DiagnosticMessage {
        code: 2344,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' does not satisfy the constraint '{1}'.",
    },
    DiagnosticMessage {
        code: 2345,
        category: DiagnosticCategory::Error,
        message: "Argument of type '{0}' is not assignable to parameter of type '{1}'.",
    },
    DiagnosticMessage {
        code: 2346,
        category: DiagnosticCategory::Error,
        message: "Call target does not contain any signatures.",
    },
    DiagnosticMessage {
        code: 2347,
        category: DiagnosticCategory::Error,
        message: "Untyped function calls may not accept type arguments.",
    },
    DiagnosticMessage {
        code: 2348,
        category: DiagnosticCategory::Error,
        message: "Value of type '{0}' is not callable. Did you mean to include 'new'?",
    },
    DiagnosticMessage {
        code: 2349,
        category: DiagnosticCategory::Error,
        message: "This expression is not callable.",
    },
    DiagnosticMessage {
        code: 2350,
        category: DiagnosticCategory::Error,
        message: "Only a void function can be called with the 'new' keyword.",
    },
    DiagnosticMessage {
        code: 2351,
        category: DiagnosticCategory::Error,
        message: "This expression is not constructable.",
    },
    DiagnosticMessage {
        code: 2352,
        category: DiagnosticCategory::Error,
        message: "Conversion of type '{0}' to type '{1}' may be a mistake because neither type sufficiently overlaps with the other. If this was intentional, convert the expression to 'unknown' first.",
    },
    DiagnosticMessage {
        code: 2353,
        category: DiagnosticCategory::Error,
        message: "Object literal may only specify known properties, and '{0}' does not exist in type '{1}'.",
    },
    DiagnosticMessage {
        code: 2354,
        category: DiagnosticCategory::Error,
        message: "This syntax requires an imported helper but module '{0}' cannot be found.",
    },
    DiagnosticMessage {
        code: 2355,
        category: DiagnosticCategory::Error,
        message: "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
    },
    DiagnosticMessage {
        code: 2356,
        category: DiagnosticCategory::Error,
        message: "An arithmetic operand must be of type 'any', 'number', 'bigint' or an enum type.",
    },
    DiagnosticMessage {
        code: 2357,
        category: DiagnosticCategory::Error,
        message: "The operand of an increment or decrement operator must be a variable or a property access.",
    },
    DiagnosticMessage {
        code: 2358,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of an 'instanceof' expression must be of type 'any', an object type or a type parameter.",
    },
    DiagnosticMessage {
        code: 2359,
        category: DiagnosticCategory::Error,
        message: "The right-hand side of an 'instanceof' expression must be either of type 'any', a class, function, or other type assignable to the 'Function' interface type, or an object type with a 'Symbol.hasInstance' method.",
    },
    DiagnosticMessage {
        code: 2362,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
    },
    DiagnosticMessage {
        code: 2363,
        category: DiagnosticCategory::Error,
        message: "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
    },
    DiagnosticMessage {
        code: 2364,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of an assignment expression must be a variable or a property access.",
    },
    DiagnosticMessage {
        code: 2365,
        category: DiagnosticCategory::Error,
        message: "Operator '{0}' cannot be applied to types '{1}' and '{2}'.",
    },
    DiagnosticMessage {
        code: 2366,
        category: DiagnosticCategory::Error,
        message: "Function lacks ending return statement and return type does not include 'undefined'.",
    },
    DiagnosticMessage {
        code: 2367,
        category: DiagnosticCategory::Error,
        message: "This comparison appears to be unintentional because the types '{0}' and '{1}' have no overlap.",
    },
    DiagnosticMessage {
        code: 2368,
        category: DiagnosticCategory::Error,
        message: "Type parameter name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2369,
        category: DiagnosticCategory::Error,
        message: "A parameter property is only allowed in a constructor implementation.",
    },
    DiagnosticMessage {
        code: 2370,
        category: DiagnosticCategory::Error,
        message: "A rest parameter must be of an array type.",
    },
    DiagnosticMessage {
        code: 2371,
        category: DiagnosticCategory::Error,
        message: "A parameter initializer is only allowed in a function or constructor implementation.",
    },
    DiagnosticMessage {
        code: 2372,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' cannot reference itself.",
    },
    DiagnosticMessage {
        code: 2373,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' cannot reference identifier '{1}' declared after it.",
    },
    DiagnosticMessage {
        code: 2374,
        category: DiagnosticCategory::Error,
        message: "Duplicate index signature for type '{0}'.",
    },
    DiagnosticMessage {
        code: 2375,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties.",
    },
    DiagnosticMessage {
        code: 2376,
        category: DiagnosticCategory::Error,
        message: "A 'super' call must be the first statement in the constructor to refer to 'super' or 'this' when a derived class contains initialized properties, parameter properties, or private identifiers.",
    },
    DiagnosticMessage {
        code: 2377,
        category: DiagnosticCategory::Error,
        message: "Constructors for derived classes must contain a 'super' call.",
    },
    DiagnosticMessage {
        code: 2378,
        category: DiagnosticCategory::Error,
        message: "A 'get' accessor must return a value.",
    },
    DiagnosticMessage {
        code: 2379,
        category: DiagnosticCategory::Error,
        message: "Argument of type '{0}' is not assignable to parameter of type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties.",
    },
    DiagnosticMessage {
        code: 2383,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be exported or non-exported.",
    },
    DiagnosticMessage {
        code: 2384,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be ambient or non-ambient.",
    },
    DiagnosticMessage {
        code: 2385,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be public, private or protected.",
    },
    DiagnosticMessage {
        code: 2386,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be optional or required.",
    },
    DiagnosticMessage {
        code: 2387,
        category: DiagnosticCategory::Error,
        message: "Function overload must be static.",
    },
    DiagnosticMessage {
        code: 2388,
        category: DiagnosticCategory::Error,
        message: "Function overload must not be static.",
    },
    DiagnosticMessage {
        code: 2389,
        category: DiagnosticCategory::Error,
        message: "Function implementation name must be '{0}'.",
    },
    DiagnosticMessage {
        code: 2390,
        category: DiagnosticCategory::Error,
        message: "Constructor implementation is missing.",
    },
    DiagnosticMessage {
        code: 2391,
        category: DiagnosticCategory::Error,
        message: "Function implementation is missing or not immediately following the declaration.",
    },
    DiagnosticMessage {
        code: 2392,
        category: DiagnosticCategory::Error,
        message: "Multiple constructor implementations are not allowed.",
    },
    DiagnosticMessage {
        code: 2393,
        category: DiagnosticCategory::Error,
        message: "Duplicate function implementation.",
    },
    DiagnosticMessage {
        code: 2394,
        category: DiagnosticCategory::Error,
        message: "This overload signature is not compatible with its implementation signature.",
    },
    DiagnosticMessage {
        code: 2395,
        category: DiagnosticCategory::Error,
        message: "Individual declarations in merged declaration '{0}' must be all exported or all local.",
    },
    DiagnosticMessage {
        code: 2396,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier 'arguments'. Compiler uses 'arguments' to initialize rest parameters.",
    },
    DiagnosticMessage {
        code: 2397,
        category: DiagnosticCategory::Error,
        message: "Declaration name conflicts with built-in global identifier '{0}'.",
    },
    DiagnosticMessage {
        code: 2398,
        category: DiagnosticCategory::Error,
        message: "'constructor' cannot be used as a parameter property name.",
    },
    DiagnosticMessage {
        code: 2399,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '_this'. Compiler uses variable declaration '_this' to capture 'this' reference.",
    },
    DiagnosticMessage {
        code: 2400,
        category: DiagnosticCategory::Error,
        message: "Expression resolves to variable declaration '_this' that compiler uses to capture 'this' reference.",
    },
    DiagnosticMessage {
        code: 2401,
        category: DiagnosticCategory::Error,
        message: "A 'super' call must be a root-level statement within a constructor of a derived class that contains initialized properties, parameter properties, or private identifiers.",
    },
    DiagnosticMessage {
        code: 2402,
        category: DiagnosticCategory::Error,
        message: "Expression resolves to '_super' that compiler uses to capture base class reference.",
    },
    DiagnosticMessage {
        code: 2403,
        category: DiagnosticCategory::Error,
        message: "Subsequent variable declarations must have the same type.  Variable '{0}' must be of type '{1}', but here has type '{2}'.",
    },
    DiagnosticMessage {
        code: 2404,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement cannot use a type annotation.",
    },
    DiagnosticMessage {
        code: 2405,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement must be of type 'string' or 'any'.",
    },
    DiagnosticMessage {
        code: 2406,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement must be a variable or a property access.",
    },
    DiagnosticMessage {
        code: 2407,
        category: DiagnosticCategory::Error,
        message: "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type '{0}'.",
    },
    DiagnosticMessage {
        code: 2408,
        category: DiagnosticCategory::Error,
        message: "Setters cannot return a value.",
    },
    DiagnosticMessage {
        code: 2409,
        category: DiagnosticCategory::Error,
        message: "Return type of constructor signature must be assignable to the instance type of the class.",
    },
    DiagnosticMessage {
        code: 2410,
        category: DiagnosticCategory::Error,
        message: "The 'with' statement is not supported. All symbols in a 'with' block will have type 'any'.",
    },
    DiagnosticMessage {
        code: 2411,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' of type '{1}' is not assignable to '{2}' index type '{3}'.",
    },
    DiagnosticMessage {
        code: 2412,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the type of the target.",
    },
    DiagnosticMessage {
        code: 2413,
        category: DiagnosticCategory::Error,
        message: "'{0}' index type '{1}' is not assignable to '{2}' index type '{3}'.",
    },
    DiagnosticMessage {
        code: 2414,
        category: DiagnosticCategory::Error,
        message: "Class name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2415,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' incorrectly extends base class '{1}'.",
    },
    DiagnosticMessage {
        code: 2416,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' in type '{1}' is not assignable to the same property in base type '{2}'.",
    },
    DiagnosticMessage {
        code: 2417,
        category: DiagnosticCategory::Error,
        message: "Class static side '{0}' incorrectly extends base class static side '{1}'.",
    },
    DiagnosticMessage {
        code: 2418,
        category: DiagnosticCategory::Error,
        message: "Type of computed property's value is '{0}', which is not assignable to type '{1}'.",
    },
    DiagnosticMessage {
        code: 2419,
        category: DiagnosticCategory::Error,
        message: "Types of construct signatures are incompatible.",
    },
    DiagnosticMessage {
        code: 2420,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' incorrectly implements interface '{1}'.",
    },
    DiagnosticMessage {
        code: 2422,
        category: DiagnosticCategory::Error,
        message: "A class can only implement an object type or intersection of object types with statically known members.",
    },
    DiagnosticMessage {
        code: 2423,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' defines instance member function '{1}', but extended class '{2}' defines it as instance member accessor.",
    },
    DiagnosticMessage {
        code: 2425,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' defines instance member property '{1}', but extended class '{2}' defines it as instance member function.",
    },
    DiagnosticMessage {
        code: 2426,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' defines instance member accessor '{1}', but extended class '{2}' defines it as instance member function.",
    },
    DiagnosticMessage {
        code: 2427,
        category: DiagnosticCategory::Error,
        message: "Interface name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2428,
        category: DiagnosticCategory::Error,
        message: "All declarations of '{0}' must have identical type parameters.",
    },
    DiagnosticMessage {
        code: 2430,
        category: DiagnosticCategory::Error,
        message: "Interface '{0}' incorrectly extends interface '{1}'.",
    },
    DiagnosticMessage {
        code: 2431,
        category: DiagnosticCategory::Error,
        message: "Enum name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2432,
        category: DiagnosticCategory::Error,
        message: "In an enum with multiple declarations, only one declaration can omit an initializer for its first enum element.",
    },
    DiagnosticMessage {
        code: 2433,
        category: DiagnosticCategory::Error,
        message: "A namespace declaration cannot be in a different file from a class or function with which it is merged.",
    },
    DiagnosticMessage {
        code: 2434,
        category: DiagnosticCategory::Error,
        message: "A namespace declaration cannot be located prior to a class or function with which it is merged.",
    },
    DiagnosticMessage {
        code: 2435,
        category: DiagnosticCategory::Error,
        message: "Ambient modules cannot be nested in other modules or namespaces.",
    },
    DiagnosticMessage {
        code: 2436,
        category: DiagnosticCategory::Error,
        message: "Ambient module declaration cannot specify relative module name.",
    },
    DiagnosticMessage {
        code: 2437,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' is hidden by a local declaration with the same name.",
    },
    DiagnosticMessage {
        code: 2438,
        category: DiagnosticCategory::Error,
        message: "Import name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2439,
        category: DiagnosticCategory::Error,
        message: "Import or export declaration in an ambient module declaration cannot reference module through relative module name.",
    },
    DiagnosticMessage {
        code: 2440,
        category: DiagnosticCategory::Error,
        message: "Import declaration conflicts with local declaration of '{0}'.",
    },
    DiagnosticMessage {
        code: 2441,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Compiler reserves name '{1}' in top level scope of a module.",
    },
    DiagnosticMessage {
        code: 2442,
        category: DiagnosticCategory::Error,
        message: "Types have separate declarations of a private property '{0}'.",
    },
    DiagnosticMessage {
        code: 2443,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected but type '{1}' is not a class derived from '{2}'.",
    },
    DiagnosticMessage {
        code: 2444,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected in type '{1}' but public in type '{2}'.",
    },
    DiagnosticMessage {
        code: 2445,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected and only accessible within class '{1}' and its subclasses.",
    },
    DiagnosticMessage {
        code: 2446,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected and only accessible through an instance of class '{1}'. This is an instance of class '{2}'.",
    },
    DiagnosticMessage {
        code: 2447,
        category: DiagnosticCategory::Error,
        message: "The '{0}' operator is not allowed for boolean types. Consider using '{1}' instead.",
    },
    DiagnosticMessage {
        code: 2448,
        category: DiagnosticCategory::Error,
        message: "Block-scoped variable '{0}' used before its declaration.",
    },
    DiagnosticMessage {
        code: 2449,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' used before its declaration.",
    },
    DiagnosticMessage {
        code: 2450,
        category: DiagnosticCategory::Error,
        message: "Enum '{0}' used before its declaration.",
    },
    DiagnosticMessage {
        code: 2451,
        category: DiagnosticCategory::Error,
        message: "Cannot redeclare block-scoped variable '{0}'.",
    },
    DiagnosticMessage {
        code: 2452,
        category: DiagnosticCategory::Error,
        message: "An enum member cannot have a numeric name.",
    },
    DiagnosticMessage {
        code: 2454,
        category: DiagnosticCategory::Error,
        message: "Variable '{0}' is used before being assigned.",
    },
    DiagnosticMessage {
        code: 2456,
        category: DiagnosticCategory::Error,
        message: "Type alias '{0}' circularly references itself.",
    },
    DiagnosticMessage {
        code: 2457,
        category: DiagnosticCategory::Error,
        message: "Type alias name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2458,
        category: DiagnosticCategory::Error,
        message: "An AMD module cannot have multiple name assignments.",
    },
    DiagnosticMessage {
        code: 2459,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' declares '{1}' locally, but it is not exported.",
    },
    DiagnosticMessage {
        code: 2460,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' declares '{1}' locally, but it is exported as '{2}'.",
    },
    DiagnosticMessage {
        code: 2461,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type.",
    },
    DiagnosticMessage {
        code: 2462,
        category: DiagnosticCategory::Error,
        message: "A rest element must be last in a destructuring pattern.",
    },
    DiagnosticMessage {
        code: 2463,
        category: DiagnosticCategory::Error,
        message: "A binding pattern parameter cannot be optional in an implementation signature.",
    },
    DiagnosticMessage {
        code: 2464,
        category: DiagnosticCategory::Error,
        message: "A computed property name must be of type 'string', 'number', 'symbol', or 'any'.",
    },
    DiagnosticMessage {
        code: 2465,
        category: DiagnosticCategory::Error,
        message: "'this' cannot be referenced in a computed property name.",
    },
    DiagnosticMessage {
        code: 2466,
        category: DiagnosticCategory::Error,
        message: "'super' cannot be referenced in a computed property name.",
    },
    DiagnosticMessage {
        code: 2467,
        category: DiagnosticCategory::Error,
        message: "A computed property name cannot reference a type parameter from its containing type.",
    },
    DiagnosticMessage {
        code: 2468,
        category: DiagnosticCategory::Error,
        message: "Cannot find global value '{0}'.",
    },
    DiagnosticMessage {
        code: 2469,
        category: DiagnosticCategory::Error,
        message: "The '{0}' operator cannot be applied to type 'symbol'.",
    },
    DiagnosticMessage {
        code: 2472,
        category: DiagnosticCategory::Error,
        message: "Spread operator in 'new' expressions is only available when targeting ECMAScript 5 and higher.",
    },
    DiagnosticMessage {
        code: 2473,
        category: DiagnosticCategory::Error,
        message: "Enum declarations must all be const or non-const.",
    },
    DiagnosticMessage {
        code: 2474,
        category: DiagnosticCategory::Error,
        message: "const enum member initializers must be constant expressions.",
    },
    DiagnosticMessage {
        code: 2475,
        category: DiagnosticCategory::Error,
        message: "'const' enums can only be used in property or index access expressions or the right hand side of an import declaration or export assignment or type query.",
    },
    DiagnosticMessage {
        code: 2476,
        category: DiagnosticCategory::Error,
        message: "A const enum member can only be accessed using a string literal.",
    },
    DiagnosticMessage {
        code: 2477,
        category: DiagnosticCategory::Error,
        message: "'const' enum member initializer was evaluated to a non-finite value.",
    },
    DiagnosticMessage {
        code: 2478,
        category: DiagnosticCategory::Error,
        message: "'const' enum member initializer was evaluated to disallowed value 'NaN'.",
    },
    DiagnosticMessage {
        code: 2480,
        category: DiagnosticCategory::Error,
        message: "'let' is not allowed to be used as a name in 'let' or 'const' declarations.",
    },
    DiagnosticMessage {
        code: 2481,
        category: DiagnosticCategory::Error,
        message: "Cannot initialize outer scoped variable '{0}' in the same scope as block scoped declaration '{1}'.",
    },
    DiagnosticMessage {
        code: 2483,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...of' statement cannot use a type annotation.",
    },
    DiagnosticMessage {
        code: 2484,
        category: DiagnosticCategory::Error,
        message: "Export declaration conflicts with exported declaration of '{0}'.",
    },
    DiagnosticMessage {
        code: 2487,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...of' statement must be a variable or a property access.",
    },
    DiagnosticMessage {
        code: 2488,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' must have a '[Symbol.iterator]()' method that returns an iterator.",
    },
    DiagnosticMessage {
        code: 2489,
        category: DiagnosticCategory::Error,
        message: "An iterator must have a 'next()' method.",
    },
    DiagnosticMessage {
        code: 2490,
        category: DiagnosticCategory::Error,
        message: "The type returned by the '{0}()' method of an iterator must have a 'value' property.",
    },
    DiagnosticMessage {
        code: 2491,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement cannot be a destructuring pattern.",
    },
    DiagnosticMessage {
        code: 2492,
        category: DiagnosticCategory::Error,
        message: "Cannot redeclare identifier '{0}' in catch clause.",
    },
    DiagnosticMessage {
        code: 2493,
        category: DiagnosticCategory::Error,
        message: "Tuple type '{0}' of length '{1}' has no element at index '{2}'.",
    },
    DiagnosticMessage {
        code: 2494,
        category: DiagnosticCategory::Error,
        message: "Using a string in a 'for...of' statement is only supported in ECMAScript 5 and higher.",
    },
    DiagnosticMessage {
        code: 2495,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type or a string type.",
    },
    DiagnosticMessage {
        code: 2496,
        category: DiagnosticCategory::Error,
        message: "The 'arguments' object cannot be referenced in an arrow function in ES5. Consider using a standard function expression.",
    },
    DiagnosticMessage {
        code: 2497,
        category: DiagnosticCategory::Error,
        message: "This module can only be referenced with ECMAScript imports/exports by turning on the '{0}' flag and referencing its default export.",
    },
    DiagnosticMessage {
        code: 2498,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' uses 'export =' and cannot be used with 'export *'.",
    },
    DiagnosticMessage {
        code: 2499,
        category: DiagnosticCategory::Error,
        message: "An interface can only extend an identifier/qualified-name with optional type arguments.",
    },
    DiagnosticMessage {
        code: 2500,
        category: DiagnosticCategory::Error,
        message: "A class can only implement an identifier/qualified-name with optional type arguments.",
    },
    DiagnosticMessage {
        code: 2501,
        category: DiagnosticCategory::Error,
        message: "A rest element cannot contain a binding pattern.",
    },
    DiagnosticMessage {
        code: 2502,
        category: DiagnosticCategory::Error,
        message: "'{0}' is referenced directly or indirectly in its own type annotation.",
    },
    DiagnosticMessage {
        code: 2503,
        category: DiagnosticCategory::Error,
        message: "Cannot find namespace '{0}'.",
    },
    DiagnosticMessage {
        code: 2504,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' must have a '[Symbol.asyncIterator]()' method that returns an async iterator.",
    },
    DiagnosticMessage {
        code: 2505,
        category: DiagnosticCategory::Error,
        message: "A generator cannot have a 'void' type annotation.",
    },
    DiagnosticMessage {
        code: 2506,
        category: DiagnosticCategory::Error,
        message: "'{0}' is referenced directly or indirectly in its own base expression.",
    },
    DiagnosticMessage {
        code: 2507,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not a constructor function type.",
    },
    DiagnosticMessage {
        code: 2508,
        category: DiagnosticCategory::Error,
        message: "No base constructor has the specified number of type arguments.",
    },
    DiagnosticMessage {
        code: 2509,
        category: DiagnosticCategory::Error,
        message: "Base constructor return type '{0}' is not an object type or intersection of object types with statically known members.",
    },
    DiagnosticMessage {
        code: 2510,
        category: DiagnosticCategory::Error,
        message: "Base constructors must all have the same return type.",
    },
    DiagnosticMessage {
        code: 2511,
        category: DiagnosticCategory::Error,
        message: "Cannot create an instance of an abstract class.",
    },
    DiagnosticMessage {
        code: 2512,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be abstract or non-abstract.",
    },
    DiagnosticMessage {
        code: 2513,
        category: DiagnosticCategory::Error,
        message: "Abstract method '{0}' in class '{1}' cannot be accessed via super expression.",
    },
    DiagnosticMessage {
        code: 2514,
        category: DiagnosticCategory::Error,
        message: "A tuple type cannot be indexed with a negative value.",
    },
    DiagnosticMessage {
        code: 2515,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class '{0}' does not implement inherited abstract member {1} from class '{2}'.",
    },
    DiagnosticMessage {
        code: 2516,
        category: DiagnosticCategory::Error,
        message: "All declarations of an abstract method must be consecutive.",
    },
    DiagnosticMessage {
        code: 2517,
        category: DiagnosticCategory::Error,
        message: "Cannot assign an abstract constructor type to a non-abstract constructor type.",
    },
    DiagnosticMessage {
        code: 2518,
        category: DiagnosticCategory::Error,
        message: "A 'this'-based type guard is not compatible with a parameter-based type guard.",
    },
    DiagnosticMessage {
        code: 2519,
        category: DiagnosticCategory::Error,
        message: "An async iterator must have a 'next()' method.",
    },
    DiagnosticMessage {
        code: 2520,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Compiler uses declaration '{1}' to support async functions.",
    },
    DiagnosticMessage {
        code: 2522,
        category: DiagnosticCategory::Error,
        message: "The 'arguments' object cannot be referenced in an async function or method in ES5. Consider using a standard function or method.",
    },
    DiagnosticMessage {
        code: 2523,
        category: DiagnosticCategory::Error,
        message: "'yield' expressions cannot be used in a parameter initializer.",
    },
    DiagnosticMessage {
        code: 2524,
        category: DiagnosticCategory::Error,
        message: "'await' expressions cannot be used in a parameter initializer.",
    },
    DiagnosticMessage {
        code: 2526,
        category: DiagnosticCategory::Error,
        message: "A 'this' type is available only in a non-static member of a class or interface.",
    },
    DiagnosticMessage {
        code: 2527,
        category: DiagnosticCategory::Error,
        message: "The inferred type of '{0}' references an inaccessible '{1}' type. A type annotation is necessary.",
    },
    DiagnosticMessage {
        code: 2528,
        category: DiagnosticCategory::Error,
        message: "A module cannot have multiple default exports.",
    },
    DiagnosticMessage {
        code: 2529,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Compiler reserves name '{1}' in top level scope of a module containing async functions.",
    },
    DiagnosticMessage {
        code: 2530,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is incompatible with index signature.",
    },
    DiagnosticMessage {
        code: 2531,
        category: DiagnosticCategory::Error,
        message: "Object is possibly 'null'.",
    },
    DiagnosticMessage {
        code: 2532,
        category: DiagnosticCategory::Error,
        message: "Object is possibly 'undefined'.",
    },
    DiagnosticMessage {
        code: 2533,
        category: DiagnosticCategory::Error,
        message: "Object is possibly 'null' or 'undefined'.",
    },
    DiagnosticMessage {
        code: 2534,
        category: DiagnosticCategory::Error,
        message: "A function returning 'never' cannot have a reachable end point.",
    },
    DiagnosticMessage {
        code: 2536,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' cannot be used to index type '{1}'.",
    },
    DiagnosticMessage {
        code: 2537,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no matching index signature for type '{1}'.",
    },
    DiagnosticMessage {
        code: 2538,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' cannot be used as an index type.",
    },
    DiagnosticMessage {
        code: 2539,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is not a variable.",
    },
    DiagnosticMessage {
        code: 2540,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a read-only property.",
    },
    DiagnosticMessage {
        code: 2542,
        category: DiagnosticCategory::Error,
        message: "Index signature in type '{0}' only permits reading.",
    },
    DiagnosticMessage {
        code: 2543,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '_newTarget'. Compiler uses variable declaration '_newTarget' to capture 'new.target' meta-property reference.",
    },
    DiagnosticMessage {
        code: 2544,
        category: DiagnosticCategory::Error,
        message: "Expression resolves to variable declaration '_newTarget' that compiler uses to capture 'new.target' meta-property reference.",
    },
    DiagnosticMessage {
        code: 2545,
        category: DiagnosticCategory::Error,
        message: "A mixin class must have a constructor with a single rest parameter of type 'any[]'.",
    },
    DiagnosticMessage {
        code: 2547,
        category: DiagnosticCategory::Error,
        message: "The type returned by the '{0}()' method of an async iterator must be a promise for a type with a 'value' property.",
    },
    DiagnosticMessage {
        code: 2548,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type or does not have a '[Symbol.iterator]()' method that returns an iterator.",
    },
    DiagnosticMessage {
        code: 2549,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type or a string type or does not have a '[Symbol.iterator]()' method that returns an iterator.",
    },
    DiagnosticMessage {
        code: 2550,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{2}' or later.",
    },
    DiagnosticMessage {
        code: 2551,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'. Did you mean '{2}'?",
    },
    DiagnosticMessage {
        code: 2552,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 2553,
        category: DiagnosticCategory::Error,
        message: "Computed values are not permitted in an enum with string valued members.",
    },
    DiagnosticMessage {
        code: 2554,
        category: DiagnosticCategory::Error,
        message: "Expected {0} arguments, but got {1}.",
    },
    DiagnosticMessage {
        code: 2555,
        category: DiagnosticCategory::Error,
        message: "Expected at least {0} arguments, but got {1}.",
    },
    DiagnosticMessage {
        code: 2556,
        category: DiagnosticCategory::Error,
        message: "A spread argument must either have a tuple type or be passed to a rest parameter.",
    },
    DiagnosticMessage {
        code: 2558,
        category: DiagnosticCategory::Error,
        message: "Expected {0} type arguments, but got {1}.",
    },
    DiagnosticMessage {
        code: 2559,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no properties in common with type '{1}'.",
    },
    DiagnosticMessage {
        code: 2560,
        category: DiagnosticCategory::Error,
        message: "Value of type '{0}' has no properties in common with type '{1}'. Did you mean to call it?",
    },
    DiagnosticMessage {
        code: 2561,
        category: DiagnosticCategory::Error,
        message: "Object literal may only specify known properties, but '{0}' does not exist in type '{1}'. Did you mean to write '{2}'?",
    },
    DiagnosticMessage {
        code: 2562,
        category: DiagnosticCategory::Error,
        message: "Base class expressions cannot reference class type parameters.",
    },
    DiagnosticMessage {
        code: 2563,
        category: DiagnosticCategory::Error,
        message: "The containing function or module body is too large for control flow analysis.",
    },
    DiagnosticMessage {
        code: 2564,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' has no initializer and is not definitely assigned in the constructor.",
    },
    DiagnosticMessage {
        code: 2565,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is used before being assigned.",
    },
    DiagnosticMessage {
        code: 2566,
        category: DiagnosticCategory::Error,
        message: "A rest element cannot have a property name.",
    },
    DiagnosticMessage {
        code: 2567,
        category: DiagnosticCategory::Error,
        message: "Enum declarations can only merge with namespace or other enum declarations.",
    },
    DiagnosticMessage {
        code: 2568,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' may not exist on type '{1}'. Did you mean '{2}'?",
    },
    DiagnosticMessage {
        code: 2570,
        category: DiagnosticCategory::Error,
        message: "Could not find name '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 2571,
        category: DiagnosticCategory::Error,
        message: "Object is of type 'unknown'.",
    },
    DiagnosticMessage {
        code: 2574,
        category: DiagnosticCategory::Error,
        message: "A rest element type must be an array type.",
    },
    DiagnosticMessage {
        code: 2575,
        category: DiagnosticCategory::Error,
        message: "No overload expects {0} arguments, but overloads do exist that expect either {1} or {2} arguments.",
    },
    DiagnosticMessage {
        code: 2576,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'. Did you mean to access the static member '{2}' instead?",
    },
    DiagnosticMessage {
        code: 2577,
        category: DiagnosticCategory::Error,
        message: "Return type annotation circularly references itself.",
    },
    DiagnosticMessage {
        code: 2578,
        category: DiagnosticCategory::Error,
        message: "Unused '@ts-expect-error' directive.",
    },
    DiagnosticMessage {
        code: 2580,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node`.",
    },
    DiagnosticMessage {
        code: 2581,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for jQuery? Try `npm i --save-dev @types/jquery`.",
    },
    DiagnosticMessage {
        code: 2582,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for a test runner? Try `npm i --save-dev @types/jest` or `npm i --save-dev @types/mocha`.",
    },
    DiagnosticMessage {
        code: 2583,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{1}' or later.",
    },
    DiagnosticMessage {
        code: 2584,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to change your target library? Try changing the 'lib' compiler option to include 'dom'.",
    },
    DiagnosticMessage {
        code: 2585,
        category: DiagnosticCategory::Error,
        message: "'{0}' only refers to a type, but is being used as a value here. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.",
    },
    DiagnosticMessage {
        code: 2588,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a constant.",
    },
    DiagnosticMessage {
        code: 2589,
        category: DiagnosticCategory::Error,
        message: "Type instantiation is excessively deep and possibly infinite.",
    },
    DiagnosticMessage {
        code: 2590,
        category: DiagnosticCategory::Error,
        message: "Expression produces a union type that is too complex to represent.",
    },
    DiagnosticMessage {
        code: 2591,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node` and then add 'node' to the types field in your tsconfig.",
    },
    DiagnosticMessage {
        code: 2592,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for jQuery? Try `npm i --save-dev @types/jquery` and then add 'jquery' to the types field in your tsconfig.",
    },
    DiagnosticMessage {
        code: 2593,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for a test runner? Try `npm i --save-dev @types/jest` or `npm i --save-dev @types/mocha` and then add 'jest' or 'mocha' to the types field in your tsconfig.",
    },
    DiagnosticMessage {
        code: 2594,
        category: DiagnosticCategory::Error,
        message: "This module is declared with 'export =', and can only be used with a default import when using the '{0}' flag.",
    },
    DiagnosticMessage {
        code: 2595,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using a default import.",
    },
    DiagnosticMessage {
        code: 2596,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by turning on the 'esModuleInterop' flag and using a default import.",
    },
    DiagnosticMessage {
        code: 2597,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using a 'require' call or by using a default import.",
    },
    DiagnosticMessage {
        code: 2598,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using a 'require' call or by turning on the 'esModuleInterop' flag and using a default import.",
    },
    DiagnosticMessage {
        code: 2602,
        category: DiagnosticCategory::Error,
        message: "JSX element implicitly has type 'any' because the global type 'JSX.Element' does not exist.",
    },
    DiagnosticMessage {
        code: 2603,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' in type '{1}' is not assignable to type '{2}'.",
    },
    DiagnosticMessage {
        code: 2604,
        category: DiagnosticCategory::Error,
        message: "JSX element type '{0}' does not have any construct or call signatures.",
    },
    DiagnosticMessage {
        code: 2606,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' of JSX spread attribute is not assignable to target property.",
    },
    DiagnosticMessage {
        code: 2607,
        category: DiagnosticCategory::Error,
        message: "JSX element class does not support attributes because it does not have a '{0}' property.",
    },
    DiagnosticMessage {
        code: 2608,
        category: DiagnosticCategory::Error,
        message: "The global type 'JSX.{0}' may not have more than one property.",
    },
    DiagnosticMessage {
        code: 2609,
        category: DiagnosticCategory::Error,
        message: "JSX spread child must be an array type.",
    },
    DiagnosticMessage {
        code: 2610,
        category: DiagnosticCategory::Error,
        message: "'{0}' is defined as an accessor in class '{1}', but is overridden here in '{2}' as an instance property.",
    },
    DiagnosticMessage {
        code: 2611,
        category: DiagnosticCategory::Error,
        message: "'{0}' is defined as a property in class '{1}', but is overridden here in '{2}' as an accessor.",
    },
    DiagnosticMessage {
        code: 2612,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' will overwrite the base property in '{1}'. If this is intentional, add an initializer. Otherwise, add a 'declare' modifier or remove the redundant declaration.",
    },
    DiagnosticMessage {
        code: 2613,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' has no default export. Did you mean to use 'import { {1} } from {0}' instead?",
    },
    DiagnosticMessage {
        code: 2614,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' has no exported member '{1}'. Did you mean to use 'import {1} from {0}' instead?",
    },
    DiagnosticMessage {
        code: 2615,
        category: DiagnosticCategory::Error,
        message: "Type of property '{0}' circularly references itself in mapped type '{1}'.",
    },
    DiagnosticMessage {
        code: 2616,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using 'import {1} = require({2})' or a default import.",
    },
    DiagnosticMessage {
        code: 2617,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using 'import {1} = require({2})' or by turning on the 'esModuleInterop' flag and using a default import.",
    },
    DiagnosticMessage {
        code: 2618,
        category: DiagnosticCategory::Error,
        message: "Source has {0} element(s) but target requires {1}.",
    },
    DiagnosticMessage {
        code: 2619,
        category: DiagnosticCategory::Error,
        message: "Source has {0} element(s) but target allows only {1}.",
    },
    DiagnosticMessage {
        code: 2620,
        category: DiagnosticCategory::Error,
        message: "Target requires {0} element(s) but source may have fewer.",
    },
    DiagnosticMessage {
        code: 2621,
        category: DiagnosticCategory::Error,
        message: "Target allows only {0} element(s) but source may have more.",
    },
    DiagnosticMessage {
        code: 2623,
        category: DiagnosticCategory::Error,
        message: "Source provides no match for required element at position {0} in target.",
    },
    DiagnosticMessage {
        code: 2624,
        category: DiagnosticCategory::Error,
        message: "Source provides no match for variadic element at position {0} in target.",
    },
    DiagnosticMessage {
        code: 2625,
        category: DiagnosticCategory::Error,
        message: "Variadic element at position {0} in source does not match element at position {1} in target.",
    },
    DiagnosticMessage {
        code: 2626,
        category: DiagnosticCategory::Error,
        message: "Type at position {0} in source is not compatible with type at position {1} in target.",
    },
    DiagnosticMessage {
        code: 2627,
        category: DiagnosticCategory::Error,
        message: "Type at positions {0} through {1} in source is not compatible with type at position {2} in target.",
    },
    DiagnosticMessage {
        code: 2628,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is an enum.",
    },
    DiagnosticMessage {
        code: 2629,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a class.",
    },
    DiagnosticMessage {
        code: 2630,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a function.",
    },
    DiagnosticMessage {
        code: 2631,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a namespace.",
    },
    DiagnosticMessage {
        code: 2632,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is an import.",
    },
    DiagnosticMessage {
        code: 2633,
        category: DiagnosticCategory::Error,
        message: "JSX property access expressions cannot include JSX namespace names",
    },
    DiagnosticMessage {
        code: 2634,
        category: DiagnosticCategory::Error,
        message: "'{0}' index signatures are incompatible.",
    },
    DiagnosticMessage {
        code: 2635,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no signatures for which the type argument list is applicable.",
    },
    DiagnosticMessage {
        code: 2636,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}' as implied by variance annotation.",
    },
    DiagnosticMessage {
        code: 2637,
        category: DiagnosticCategory::Error,
        message: "Variance annotations are only supported in type aliases for object, function, constructor, and mapped types.",
    },
    DiagnosticMessage {
        code: 2638,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' may represent a primitive value, which is not permitted as the right operand of the 'in' operator.",
    },
    DiagnosticMessage {
        code: 2639,
        category: DiagnosticCategory::Error,
        message: "React components cannot include JSX namespace names",
    },
    DiagnosticMessage {
        code: 2649,
        category: DiagnosticCategory::Error,
        message: "Cannot augment module '{0}' with value exports because it resolves to a non-module entity.",
    },
    DiagnosticMessage {
        code: 2650,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class expression is missing implementations for the following members of '{0}': {1} and {2} more.",
    },
    DiagnosticMessage {
        code: 2651,
        category: DiagnosticCategory::Error,
        message: "A member initializer in a enum declaration cannot reference members declared after it, including members defined in other enums.",
    },
    DiagnosticMessage {
        code: 2652,
        category: DiagnosticCategory::Error,
        message: "Merged declaration '{0}' cannot include a default export declaration. Consider adding a separate 'export default {0}' declaration instead.",
    },
    DiagnosticMessage {
        code: 2653,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class expression does not implement inherited abstract member '{0}' from class '{1}'.",
    },
    DiagnosticMessage {
        code: 2654,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class '{0}' is missing implementations for the following members of '{1}': {2}.",
    },
    DiagnosticMessage {
        code: 2655,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class '{0}' is missing implementations for the following members of '{1}': {2} and {3} more.",
    },
    DiagnosticMessage {
        code: 2656,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class expression is missing implementations for the following members of '{0}': {1}.",
    },
    DiagnosticMessage {
        code: 2657,
        category: DiagnosticCategory::Error,
        message: "JSX expressions must have one parent element.",
    },
    DiagnosticMessage {
        code: 2658,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' provides no match for the signature '{1}'.",
    },
    DiagnosticMessage {
        code: 2659,
        category: DiagnosticCategory::Error,
        message: "'super' is only allowed in members of object literal expressions when option 'target' is 'ES2015' or higher.",
    },
    DiagnosticMessage {
        code: 2660,
        category: DiagnosticCategory::Error,
        message: "'super' can only be referenced in members of derived classes or object literal expressions.",
    },
    DiagnosticMessage {
        code: 2661,
        category: DiagnosticCategory::Error,
        message: "Cannot export '{0}'. Only local declarations can be exported from a module.",
    },
    DiagnosticMessage {
        code: 2662,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Did you mean the static member '{1}.{0}'?",
    },
    DiagnosticMessage {
        code: 2663,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Did you mean the instance member 'this.{0}'?",
    },
    DiagnosticMessage {
        code: 2664,
        category: DiagnosticCategory::Error,
        message: "Invalid module name in augmentation, module '{0}' cannot be found.",
    },
    DiagnosticMessage {
        code: 2665,
        category: DiagnosticCategory::Error,
        message: "Invalid module name in augmentation. Module '{0}' resolves to an untyped module at '{1}', which cannot be augmented.",
    },
    DiagnosticMessage {
        code: 2666,
        category: DiagnosticCategory::Error,
        message: "Exports and export assignments are not permitted in module augmentations.",
    },
    DiagnosticMessage {
        code: 2667,
        category: DiagnosticCategory::Error,
        message: "Imports are not permitted in module augmentations. Consider moving them to the enclosing external module.",
    },
    DiagnosticMessage {
        code: 2668,
        category: DiagnosticCategory::Error,
        message: "'export' modifier cannot be applied to ambient modules and module augmentations since they are always visible.",
    },
    DiagnosticMessage {
        code: 2669,
        category: DiagnosticCategory::Error,
        message: "Augmentations for the global scope can only be directly nested in external modules or ambient module declarations.",
    },
    DiagnosticMessage {
        code: 2670,
        category: DiagnosticCategory::Error,
        message: "Augmentations for the global scope should have 'declare' modifier unless they appear in already ambient context.",
    },
    DiagnosticMessage {
        code: 2671,
        category: DiagnosticCategory::Error,
        message: "Cannot augment module '{0}' because it resolves to a non-module entity.",
    },
    DiagnosticMessage {
        code: 2672,
        category: DiagnosticCategory::Error,
        message: "Cannot assign a '{0}' constructor type to a '{1}' constructor type.",
    },
    DiagnosticMessage {
        code: 2673,
        category: DiagnosticCategory::Error,
        message: "Constructor of class '{0}' is private and only accessible within the class declaration.",
    },
    DiagnosticMessage {
        code: 2674,
        category: DiagnosticCategory::Error,
        message: "Constructor of class '{0}' is protected and only accessible within the class declaration.",
    },
    DiagnosticMessage {
        code: 2675,
        category: DiagnosticCategory::Error,
        message: "Cannot extend a class '{0}'. Class constructor is marked as private.",
    },
    DiagnosticMessage {
        code: 2676,
        category: DiagnosticCategory::Error,
        message: "Accessors must both be abstract or non-abstract.",
    },
    DiagnosticMessage {
        code: 2677,
        category: DiagnosticCategory::Error,
        message: "A type predicate's type must be assignable to its parameter's type.",
    },
    DiagnosticMessage {
        code: 2678,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not comparable to type '{1}'.",
    },
    DiagnosticMessage {
        code: 2679,
        category: DiagnosticCategory::Error,
        message: "A function that is called with the 'new' keyword cannot have a 'this' type that is 'void'.",
    },
    DiagnosticMessage {
        code: 2680,
        category: DiagnosticCategory::Error,
        message: "A '{0}' parameter must be the first parameter.",
    },
    DiagnosticMessage {
        code: 2681,
        category: DiagnosticCategory::Error,
        message: "A constructor cannot have a 'this' parameter.",
    },
    DiagnosticMessage {
        code: 2683,
        category: DiagnosticCategory::Error,
        message: "'this' implicitly has type 'any' because it does not have a type annotation.",
    },
    DiagnosticMessage {
        code: 2684,
        category: DiagnosticCategory::Error,
        message: "The 'this' context of type '{0}' is not assignable to method's 'this' of type '{1}'.",
    },
    DiagnosticMessage {
        code: 2685,
        category: DiagnosticCategory::Error,
        message: "The 'this' types of each signature are incompatible.",
    },
    DiagnosticMessage {
        code: 2686,
        category: DiagnosticCategory::Error,
        message: "'{0}' refers to a UMD global, but the current file is a module. Consider adding an import instead.",
    },
    DiagnosticMessage {
        code: 2687,
        category: DiagnosticCategory::Error,
        message: "All declarations of '{0}' must have identical modifiers.",
    },
    DiagnosticMessage {
        code: 2688,
        category: DiagnosticCategory::Error,
        message: "Cannot find type definition file for '{0}'.",
    },
    DiagnosticMessage {
        code: 2689,
        category: DiagnosticCategory::Error,
        message: "Cannot extend an interface '{0}'. Did you mean 'implements'?",
    },
    DiagnosticMessage {
        code: 2690,
        category: DiagnosticCategory::Error,
        message: "'{0}' only refers to a type, but is being used as a value here. Did you mean to use '{1} in {0}'?",
    },
    DiagnosticMessage {
        code: 2692,
        category: DiagnosticCategory::Error,
        message: "'{0}' is a primitive, but '{1}' is a wrapper object. Prefer using '{0}' when possible.",
    },
    DiagnosticMessage {
        code: 2693,
        category: DiagnosticCategory::Error,
        message: "'{0}' only refers to a type, but is being used as a value here.",
    },
    DiagnosticMessage {
        code: 2694,
        category: DiagnosticCategory::Error,
        message: "Namespace '{0}' has no exported member '{1}'.",
    },
    DiagnosticMessage {
        code: 2695,
        category: DiagnosticCategory::Error,
        message: "Left side of comma operator is unused and has no side effects.",
    },
    DiagnosticMessage {
        code: 2696,
        category: DiagnosticCategory::Error,
        message: "The 'Object' type is assignable to very few other types. Did you mean to use the 'any' type instead?",
    },
    DiagnosticMessage {
        code: 2697,
        category: DiagnosticCategory::Error,
        message: "An async function or method must return a 'Promise'. Make sure you have a declaration for 'Promise' or include 'ES2015' in your '--lib' option.",
    },
    DiagnosticMessage {
        code: 2698,
        category: DiagnosticCategory::Error,
        message: "Spread types may only be created from object types.",
    },
    DiagnosticMessage {
        code: 2699,
        category: DiagnosticCategory::Error,
        message: "Static property '{0}' conflicts with built-in property 'Function.{0}' of constructor function '{1}'.",
    },
    DiagnosticMessage {
        code: 2700,
        category: DiagnosticCategory::Error,
        message: "Rest types may only be created from object types.",
    },
    DiagnosticMessage {
        code: 2701,
        category: DiagnosticCategory::Error,
        message: "The target of an object rest assignment must be a variable or a property access.",
    },
    DiagnosticMessage {
        code: 2702,
        category: DiagnosticCategory::Error,
        message: "'{0}' only refers to a type, but is being used as a namespace here.",
    },
    DiagnosticMessage {
        code: 2703,
        category: DiagnosticCategory::Error,
        message: "The operand of a 'delete' operator must be a property reference.",
    },
    DiagnosticMessage {
        code: 2704,
        category: DiagnosticCategory::Error,
        message: "The operand of a 'delete' operator cannot be a read-only property.",
    },
    DiagnosticMessage {
        code: 2705,
        category: DiagnosticCategory::Error,
        message: "An async function or method in ES5 requires the 'Promise' constructor.  Make sure you have a declaration for the 'Promise' constructor or include 'ES2015' in your '--lib' option.",
    },
    DiagnosticMessage {
        code: 2706,
        category: DiagnosticCategory::Error,
        message: "Required type parameters may not follow optional type parameters.",
    },
    DiagnosticMessage {
        code: 2707,
        category: DiagnosticCategory::Error,
        message: "Generic type '{0}' requires between {1} and {2} type arguments.",
    },
    DiagnosticMessage {
        code: 2708,
        category: DiagnosticCategory::Error,
        message: "Cannot use namespace '{0}' as a value.",
    },
    DiagnosticMessage {
        code: 2709,
        category: DiagnosticCategory::Error,
        message: "Cannot use namespace '{0}' as a type.",
    },
    DiagnosticMessage {
        code: 2710,
        category: DiagnosticCategory::Error,
        message: "'{0}' are specified twice. The attribute named '{0}' will be overwritten.",
    },
    DiagnosticMessage {
        code: 2711,
        category: DiagnosticCategory::Error,
        message: "A dynamic import call returns a 'Promise'. Make sure you have a declaration for 'Promise' or include 'ES2015' in your '--lib' option.",
    },
    DiagnosticMessage {
        code: 2712,
        category: DiagnosticCategory::Error,
        message: "A dynamic import call in ES5 requires the 'Promise' constructor.  Make sure you have a declaration for the 'Promise' constructor or include 'ES2015' in your '--lib' option.",
    },
    DiagnosticMessage {
        code: 2713,
        category: DiagnosticCategory::Error,
        message: "Cannot access '{0}.{1}' because '{0}' is a type, but not a namespace. Did you mean to retrieve the type of the property '{1}' in '{0}' with '{0}[\"{1}\"]'?",
    },
    DiagnosticMessage {
        code: 2714,
        category: DiagnosticCategory::Error,
        message: "The expression of an export assignment must be an identifier or qualified name in an ambient context.",
    },
    DiagnosticMessage {
        code: 2715,
        category: DiagnosticCategory::Error,
        message: "Abstract property '{0}' in class '{1}' cannot be accessed in the constructor.",
    },
    DiagnosticMessage {
        code: 2716,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' has a circular default.",
    },
    DiagnosticMessage {
        code: 2717,
        category: DiagnosticCategory::Error,
        message: "Subsequent property declarations must have the same type.  Property '{0}' must be of type '{1}', but here has type '{2}'.",
    },
    DiagnosticMessage {
        code: 2718,
        category: DiagnosticCategory::Error,
        message: "Duplicate property '{0}'.",
    },
    DiagnosticMessage {
        code: 2719,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}'. Two different types with this name exist, but they are unrelated.",
    },
    DiagnosticMessage {
        code: 2720,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' incorrectly implements class '{1}'. Did you mean to extend '{1}' and inherit its members as a subclass?",
    },
    DiagnosticMessage {
        code: 2721,
        category: DiagnosticCategory::Error,
        message: "Cannot invoke an object which is possibly 'null'.",
    },
    DiagnosticMessage {
        code: 2722,
        category: DiagnosticCategory::Error,
        message: "Cannot invoke an object which is possibly 'undefined'.",
    },
    DiagnosticMessage {
        code: 2723,
        category: DiagnosticCategory::Error,
        message: "Cannot invoke an object which is possibly 'null' or 'undefined'.",
    },
    DiagnosticMessage {
        code: 2724,
        category: DiagnosticCategory::Error,
        message: "'{0}' has no exported member named '{1}'. Did you mean '{2}'?",
    },
    DiagnosticMessage {
        code: 2725,
        category: DiagnosticCategory::Error,
        message: "Class name cannot be 'Object' when targeting ES5 and above with module {0}.",
    },
    DiagnosticMessage {
        code: 2726,
        category: DiagnosticCategory::Error,
        message: "Cannot find lib definition for '{0}'.",
    },
    DiagnosticMessage {
        code: 2727,
        category: DiagnosticCategory::Error,
        message: "Cannot find lib definition for '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 2728,
        category: DiagnosticCategory::Message,
        message: "'{0}' is declared here.",
    },
    DiagnosticMessage {
        code: 2729,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is used before its initialization.",
    },
    DiagnosticMessage {
        code: 2730,
        category: DiagnosticCategory::Error,
        message: "An arrow function cannot have a 'this' parameter.",
    },
    DiagnosticMessage {
        code: 2731,
        category: DiagnosticCategory::Error,
        message: "Implicit conversion of a 'symbol' to a 'string' will fail at runtime. Consider wrapping this expression in 'String(...)'.",
    },
    DiagnosticMessage {
        code: 2732,
        category: DiagnosticCategory::Error,
        message: "Cannot find module '{0}'. Consider using '--resolveJsonModule' to import module with '.json' extension.",
    },
    DiagnosticMessage {
        code: 2733,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' was also declared here.",
    },
    DiagnosticMessage {
        code: 2734,
        category: DiagnosticCategory::Error,
        message: "Are you missing a semicolon?",
    },
    DiagnosticMessage {
        code: 2735,
        category: DiagnosticCategory::Error,
        message: "Did you mean for '{0}' to be constrained to type 'new (...args: any[]) => {1}'?",
    },
    DiagnosticMessage {
        code: 2736,
        category: DiagnosticCategory::Error,
        message: "Operator '{0}' cannot be applied to type '{1}'.",
    },
    DiagnosticMessage {
        code: 2737,
        category: DiagnosticCategory::Error,
        message: "BigInt literals are not available when targeting lower than ES2020.",
    },
    DiagnosticMessage {
        code: 2738,
        category: DiagnosticCategory::Message,
        message: "An outer value of 'this' is shadowed by this container.",
    },
    DiagnosticMessage {
        code: 2739,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is missing the following properties from type '{1}': {2}",
    },
    DiagnosticMessage {
        code: 2740,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is missing the following properties from type '{1}': {2}, and {3} more.",
    },
    DiagnosticMessage {
        code: 2741,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is missing in type '{1}' but required in type '{2}'.",
    },
    DiagnosticMessage {
        code: 2742,
        category: DiagnosticCategory::Error,
        message: "The inferred type of '{0}' cannot be named without a reference to '{1}'. This is likely not portable. A type annotation is necessary.",
    },
    DiagnosticMessage {
        code: 2743,
        category: DiagnosticCategory::Error,
        message: "No overload expects {0} type arguments, but overloads do exist that expect either {1} or {2} type arguments.",
    },
    DiagnosticMessage {
        code: 2744,
        category: DiagnosticCategory::Error,
        message: "Type parameter defaults can only reference previously declared type parameters.",
    },
    DiagnosticMessage {
        code: 2745,
        category: DiagnosticCategory::Error,
        message: "This JSX tag's '{0}' prop expects type '{1}' which requires multiple children, but only a single child was provided.",
    },
    DiagnosticMessage {
        code: 2746,
        category: DiagnosticCategory::Error,
        message: "This JSX tag's '{0}' prop expects a single child of type '{1}', but multiple children were provided.",
    },
    DiagnosticMessage {
        code: 2747,
        category: DiagnosticCategory::Error,
        message: "'{0}' components don't accept text as child elements. Text in JSX has the type 'string', but the expected type of '{1}' is '{2}'.",
    },
    DiagnosticMessage {
        code: 2748,
        category: DiagnosticCategory::Error,
        message: "Cannot access ambient const enums when '{0}' is enabled.",
    },
    DiagnosticMessage {
        code: 2749,
        category: DiagnosticCategory::Error,
        message: "'{0}' refers to a value, but is being used as a type here. Did you mean 'typeof {0}'?",
    },
    DiagnosticMessage {
        code: 2750,
        category: DiagnosticCategory::Error,
        message: "The implementation signature is declared here.",
    },
    DiagnosticMessage {
        code: 2751,
        category: DiagnosticCategory::Error,
        message: "Circularity originates in type at this location.",
    },
    DiagnosticMessage {
        code: 2752,
        category: DiagnosticCategory::Error,
        message: "The first export default is here.",
    },
    DiagnosticMessage {
        code: 2753,
        category: DiagnosticCategory::Error,
        message: "Another export default is here.",
    },
    DiagnosticMessage {
        code: 2754,
        category: DiagnosticCategory::Error,
        message: "'super' may not use type arguments.",
    },
    DiagnosticMessage {
        code: 2755,
        category: DiagnosticCategory::Error,
        message: "No constituent of type '{0}' is callable.",
    },
    DiagnosticMessage {
        code: 2756,
        category: DiagnosticCategory::Error,
        message: "Not all constituents of type '{0}' are callable.",
    },
    DiagnosticMessage {
        code: 2757,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no call signatures.",
    },
    DiagnosticMessage {
        code: 2758,
        category: DiagnosticCategory::Error,
        message: "Each member of the union type '{0}' has signatures, but none of those signatures are compatible with each other.",
    },
    DiagnosticMessage {
        code: 2759,
        category: DiagnosticCategory::Error,
        message: "No constituent of type '{0}' is constructable.",
    },
    DiagnosticMessage {
        code: 2760,
        category: DiagnosticCategory::Error,
        message: "Not all constituents of type '{0}' are constructable.",
    },
    DiagnosticMessage {
        code: 2761,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no construct signatures.",
    },
    DiagnosticMessage {
        code: 2762,
        category: DiagnosticCategory::Error,
        message: "Each member of the union type '{0}' has construct signatures, but none of those signatures are compatible with each other.",
    },
    DiagnosticMessage {
        code: 2763,
        category: DiagnosticCategory::Error,
        message: "Cannot iterate value because the 'next' method of its iterator expects type '{1}', but for-of will always send '{0}'.",
    },
    DiagnosticMessage {
        code: 2764,
        category: DiagnosticCategory::Error,
        message: "Cannot iterate value because the 'next' method of its iterator expects type '{1}', but array spread will always send '{0}'.",
    },
    DiagnosticMessage {
        code: 2765,
        category: DiagnosticCategory::Error,
        message: "Cannot iterate value because the 'next' method of its iterator expects type '{1}', but array destructuring will always send '{0}'.",
    },
    DiagnosticMessage {
        code: 2766,
        category: DiagnosticCategory::Error,
        message: "Cannot delegate iteration to value because the 'next' method of its iterator expects type '{1}', but the containing generator will always send '{0}'.",
    },
    DiagnosticMessage {
        code: 2767,
        category: DiagnosticCategory::Error,
        message: "The '{0}' property of an iterator must be a method.",
    },
    DiagnosticMessage {
        code: 2768,
        category: DiagnosticCategory::Error,
        message: "The '{0}' property of an async iterator must be a method.",
    },
    DiagnosticMessage {
        code: 2769,
        category: DiagnosticCategory::Error,
        message: "No overload matches this call.",
    },
    DiagnosticMessage {
        code: 2770,
        category: DiagnosticCategory::Error,
        message: "The last overload gave the following error.",
    },
    DiagnosticMessage {
        code: 2771,
        category: DiagnosticCategory::Error,
        message: "The last overload is declared here.",
    },
    DiagnosticMessage {
        code: 2772,
        category: DiagnosticCategory::Error,
        message: "Overload {0} of {1}, '{2}', gave the following error.",
    },
    DiagnosticMessage {
        code: 2773,
        category: DiagnosticCategory::Error,
        message: "Did you forget to use 'await'?",
    },
    DiagnosticMessage {
        code: 2774,
        category: DiagnosticCategory::Error,
        message: "This condition will always return true since this function is always defined. Did you mean to call it instead?",
    },
    DiagnosticMessage {
        code: 2775,
        category: DiagnosticCategory::Error,
        message: "Assertions require every name in the call target to be declared with an explicit type annotation.",
    },
    DiagnosticMessage {
        code: 2776,
        category: DiagnosticCategory::Error,
        message: "Assertions require the call target to be an identifier or qualified name.",
    },
    DiagnosticMessage {
        code: 2777,
        category: DiagnosticCategory::Error,
        message: "The operand of an increment or decrement operator may not be an optional property access.",
    },
    DiagnosticMessage {
        code: 2778,
        category: DiagnosticCategory::Error,
        message: "The target of an object rest assignment may not be an optional property access.",
    },
    DiagnosticMessage {
        code: 2779,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of an assignment expression may not be an optional property access.",
    },
    DiagnosticMessage {
        code: 2780,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement may not be an optional property access.",
    },
    DiagnosticMessage {
        code: 2781,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...of' statement may not be an optional property access.",
    },
    DiagnosticMessage {
        code: 2782,
        category: DiagnosticCategory::Message,
        message: "'{0}' needs an explicit type annotation.",
    },
    DiagnosticMessage {
        code: 2783,
        category: DiagnosticCategory::Error,
        message: "'{0}' is specified more than once, so this usage will be overwritten.",
    },
    DiagnosticMessage {
        code: 2784,
        category: DiagnosticCategory::Error,
        message: "'get' and 'set' accessors cannot declare 'this' parameters.",
    },
    DiagnosticMessage {
        code: 2785,
        category: DiagnosticCategory::Error,
        message: "This spread always overwrites this property.",
    },
    DiagnosticMessage {
        code: 2786,
        category: DiagnosticCategory::Error,
        message: "'{0}' cannot be used as a JSX component.",
    },
    DiagnosticMessage {
        code: 2787,
        category: DiagnosticCategory::Error,
        message: "Its return type '{0}' is not a valid JSX element.",
    },
    DiagnosticMessage {
        code: 2788,
        category: DiagnosticCategory::Error,
        message: "Its instance type '{0}' is not a valid JSX element.",
    },
    DiagnosticMessage {
        code: 2789,
        category: DiagnosticCategory::Error,
        message: "Its element type '{0}' is not a valid JSX element.",
    },
    DiagnosticMessage {
        code: 2790,
        category: DiagnosticCategory::Error,
        message: "The operand of a 'delete' operator must be optional.",
    },
    DiagnosticMessage {
        code: 2791,
        category: DiagnosticCategory::Error,
        message: "Exponentiation cannot be performed on 'bigint' values unless the 'target' option is set to 'es2016' or later.",
    },
    DiagnosticMessage {
        code: 2792,
        category: DiagnosticCategory::Error,
        message: "Cannot find module '{0}'. Did you mean to set the 'moduleResolution' option to 'nodenext', or to add aliases to the 'paths' option?",
    },
    DiagnosticMessage {
        code: 2793,
        category: DiagnosticCategory::Error,
        message: "The call would have succeeded against this implementation, but implementation signatures of overloads are not externally visible.",
    },
    DiagnosticMessage {
        code: 2794,
        category: DiagnosticCategory::Error,
        message: "Expected {0} arguments, but got {1}. Did you forget to include 'void' in your type argument to 'Promise'?",
    },
    DiagnosticMessage {
        code: 2795,
        category: DiagnosticCategory::Error,
        message: "The 'intrinsic' keyword can only be used to declare compiler provided intrinsic types.",
    },
    DiagnosticMessage {
        code: 2796,
        category: DiagnosticCategory::Error,
        message: "It is likely that you are missing a comma to separate these two template expressions. They form a tagged template expression which cannot be invoked.",
    },
    DiagnosticMessage {
        code: 2797,
        category: DiagnosticCategory::Error,
        message: "A mixin class that extends from a type variable containing an abstract construct signature must also be declared 'abstract'.",
    },
    DiagnosticMessage {
        code: 2798,
        category: DiagnosticCategory::Error,
        message: "The declaration was marked as deprecated here.",
    },
    DiagnosticMessage {
        code: 2799,
        category: DiagnosticCategory::Error,
        message: "Type produces a tuple type that is too large to represent.",
    },
    DiagnosticMessage {
        code: 2800,
        category: DiagnosticCategory::Error,
        message: "Expression produces a tuple type that is too large to represent.",
    },
    DiagnosticMessage {
        code: 2801,
        category: DiagnosticCategory::Error,
        message: "This condition will always return true since this '{0}' is always defined.",
    },
    DiagnosticMessage {
        code: 2802,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' can only be iterated through when using the '--downlevelIteration' flag or with a '--target' of 'es2015' or higher.",
    },
    DiagnosticMessage {
        code: 2803,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to private method '{0}'. Private methods are not writable.",
    },
    DiagnosticMessage {
        code: 2804,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Static and instance elements cannot share the same private name.",
    },
    DiagnosticMessage {
        code: 2806,
        category: DiagnosticCategory::Error,
        message: "Private accessor was defined without a getter.",
    },
    DiagnosticMessage {
        code: 2807,
        category: DiagnosticCategory::Error,
        message: "This syntax requires an imported helper named '{1}' with {2} parameters, which is not compatible with the one in '{0}'. Consider upgrading your version of '{0}'.",
    },
    DiagnosticMessage {
        code: 2808,
        category: DiagnosticCategory::Error,
        message: "A get accessor must be at least as accessible as the setter",
    },
    DiagnosticMessage {
        code: 2809,
        category: DiagnosticCategory::Error,
        message: "Declaration or statement expected. This '=' follows a block of statements, so if you intended to write a destructuring assignment, you might need to wrap the whole assignment in parentheses.",
    },
    DiagnosticMessage {
        code: 2810,
        category: DiagnosticCategory::Error,
        message: "Expected 1 argument, but got 0. 'new Promise()' needs a JSDoc hint to produce a 'resolve' that can be called without arguments.",
    },
    DiagnosticMessage {
        code: 2811,
        category: DiagnosticCategory::Error,
        message: "Initializer for property '{0}'",
    },
    DiagnosticMessage {
        code: 2812,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'. Try changing the 'lib' compiler option to include 'dom'.",
    },
    DiagnosticMessage {
        code: 2813,
        category: DiagnosticCategory::Error,
        message: "Class declaration cannot implement overload list for '{0}'.",
    },
    DiagnosticMessage {
        code: 2814,
        category: DiagnosticCategory::Error,
        message: "Function with bodies can only merge with classes that are ambient.",
    },
    DiagnosticMessage {
        code: 2815,
        category: DiagnosticCategory::Error,
        message: "'arguments' cannot be referenced in property initializers or class static initialization blocks.",
    },
    DiagnosticMessage {
        code: 2816,
        category: DiagnosticCategory::Error,
        message: "Cannot use 'this' in a static property initializer of a decorated class.",
    },
    DiagnosticMessage {
        code: 2817,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' has no initializer and is not definitely assigned in a class static block.",
    },
    DiagnosticMessage {
        code: 2818,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Compiler reserves name '{1}' when emitting 'super' references in static initializers.",
    },
    DiagnosticMessage {
        code: 2819,
        category: DiagnosticCategory::Error,
        message: "Namespace name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2820,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}'. Did you mean '{2}'?",
    },
    DiagnosticMessage {
        code: 2821,
        category: DiagnosticCategory::Error,
        message: "Import assertions are only supported when the '--module' option is set to 'esnext', 'node18', 'node20', 'nodenext', or 'preserve'.",
    },
    DiagnosticMessage {
        code: 2822,
        category: DiagnosticCategory::Error,
        message: "Import assertions cannot be used with type-only imports or exports.",
    },
    DiagnosticMessage {
        code: 2823,
        category: DiagnosticCategory::Error,
        message: "Import attributes are only supported when the '--module' option is set to 'esnext', 'node18', 'node20', 'nodenext', or 'preserve'.",
    },
    DiagnosticMessage {
        code: 2833,
        category: DiagnosticCategory::Error,
        message: "Cannot find namespace '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 2834,
        category: DiagnosticCategory::Error,
        message: "Relative import paths need explicit file extensions in ECMAScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Consider adding an extension to the import path.",
    },
    DiagnosticMessage {
        code: 2835,
        category: DiagnosticCategory::Error,
        message: "Relative import paths need explicit file extensions in ECMAScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Did you mean '{0}'?",
    },
    DiagnosticMessage {
        code: 2836,
        category: DiagnosticCategory::Error,
        message: "Import assertions are not allowed on statements that compile to CommonJS 'require' calls.",
    },
    DiagnosticMessage {
        code: 2837,
        category: DiagnosticCategory::Error,
        message: "Import assertion values must be string literal expressions.",
    },
    DiagnosticMessage {
        code: 2838,
        category: DiagnosticCategory::Error,
        message: "All declarations of '{0}' must have identical constraints.",
    },
    DiagnosticMessage {
        code: 2839,
        category: DiagnosticCategory::Error,
        message: "This condition will always return '{0}' since JavaScript compares objects by reference, not value.",
    },
    DiagnosticMessage {
        code: 2840,
        category: DiagnosticCategory::Error,
        message: "An interface cannot extend a primitive type like '{0}'. It can only extend other named object types.",
    },
    DiagnosticMessage {
        code: 2842,
        category: DiagnosticCategory::Error,
        message: "'{0}' is an unused renaming of '{1}'. Did you intend to use it as a type annotation?",
    },
    DiagnosticMessage {
        code: 2843,
        category: DiagnosticCategory::Error,
        message: "We can only write a type for '{0}' by adding a type for the entire parameter here.",
    },
    DiagnosticMessage {
        code: 2844,
        category: DiagnosticCategory::Error,
        message: "Type of instance member variable '{0}' cannot reference identifier '{1}' declared in the constructor.",
    },
    DiagnosticMessage {
        code: 2845,
        category: DiagnosticCategory::Error,
        message: "This condition will always return '{0}'.",
    },
    DiagnosticMessage {
        code: 2846,
        category: DiagnosticCategory::Error,
        message: "A declaration file cannot be imported without 'import type'. Did you mean to import an implementation file '{0}' instead?",
    },
    DiagnosticMessage {
        code: 2848,
        category: DiagnosticCategory::Error,
        message: "The right-hand side of an 'instanceof' expression must not be an instantiation expression.",
    },
    DiagnosticMessage {
        code: 2849,
        category: DiagnosticCategory::Error,
        message: "Target signature provides too few arguments. Expected {0} or more, but got {1}.",
    },
    DiagnosticMessage {
        code: 2850,
        category: DiagnosticCategory::Error,
        message: "The initializer of a 'using' declaration must be either an object with a '[Symbol.dispose]()' method, or be 'null' or 'undefined'.",
    },
    DiagnosticMessage {
        code: 2851,
        category: DiagnosticCategory::Error,
        message: "The initializer of an 'await using' declaration must be either an object with a '[Symbol.asyncDispose]()' or '[Symbol.dispose]()' method, or be 'null' or 'undefined'.",
    },
    DiagnosticMessage {
        code: 2852,
        category: DiagnosticCategory::Error,
        message: "'await using' statements are only allowed within async functions and at the top levels of modules.",
    },
    DiagnosticMessage {
        code: 2853,
        category: DiagnosticCategory::Error,
        message: "'await using' statements are only allowed at the top level of a file when that file is a module, but this file has no imports or exports. Consider adding an empty 'export {}' to make this file a module.",
    },
    DiagnosticMessage {
        code: 2854,
        category: DiagnosticCategory::Error,
        message: "Top-level 'await using' statements are only allowed when the 'module' option is set to 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', 'nodenext', or 'preserve', and the 'target' option is set to 'es2017' or higher.",
    },
    DiagnosticMessage {
        code: 2855,
        category: DiagnosticCategory::Error,
        message: "Class field '{0}' defined by the parent class is not accessible in the child class via super.",
    },
    DiagnosticMessage {
        code: 2856,
        category: DiagnosticCategory::Error,
        message: "Import attributes are not allowed on statements that compile to CommonJS 'require' calls.",
    },
    DiagnosticMessage {
        code: 2857,
        category: DiagnosticCategory::Error,
        message: "Import attributes cannot be used with type-only imports or exports.",
    },
    DiagnosticMessage {
        code: 2858,
        category: DiagnosticCategory::Error,
        message: "Import attribute values must be string literal expressions.",
    },
    DiagnosticMessage {
        code: 2859,
        category: DiagnosticCategory::Error,
        message: "Excessive complexity comparing types '{0}' and '{1}'.",
    },
    DiagnosticMessage {
        code: 2860,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of an 'instanceof' expression must be assignable to the first argument of the right-hand side's '[Symbol.hasInstance]' method.",
    },
    DiagnosticMessage {
        code: 2861,
        category: DiagnosticCategory::Error,
        message: "An object's '[Symbol.hasInstance]' method must return a boolean value for it to be used on the right-hand side of an 'instanceof' expression.",
    },
    DiagnosticMessage {
        code: 2862,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is generic and can only be indexed for reading.",
    },
    DiagnosticMessage {
        code: 2863,
        category: DiagnosticCategory::Error,
        message: "A class cannot extend a primitive type like '{0}'. Classes can only extend constructable values.",
    },
    DiagnosticMessage {
        code: 2864,
        category: DiagnosticCategory::Error,
        message: "A class cannot implement a primitive type like '{0}'. It can only implement other named object types.",
    },
    DiagnosticMessage {
        code: 2865,
        category: DiagnosticCategory::Error,
        message: "Import '{0}' conflicts with local value, so must be declared with a type-only import when 'isolatedModules' is enabled.",
    },
    DiagnosticMessage {
        code: 2866,
        category: DiagnosticCategory::Error,
        message: "Import '{0}' conflicts with global value used in this file, so must be declared with a type-only import when 'isolatedModules' is enabled.",
    },
    DiagnosticMessage {
        code: 2867,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for Bun? Try `npm i --save-dev @types/bun`.",
    },
    DiagnosticMessage {
        code: 2868,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for Bun? Try `npm i --save-dev @types/bun` and then add 'bun' to the types field in your tsconfig.",
    },
    DiagnosticMessage {
        code: 2869,
        category: DiagnosticCategory::Error,
        message: "Right operand of ?? is unreachable because the left operand is never nullish.",
    },
    DiagnosticMessage {
        code: 2870,
        category: DiagnosticCategory::Error,
        message: "This binary expression is never nullish. Are you missing parentheses?",
    },
    DiagnosticMessage {
        code: 2871,
        category: DiagnosticCategory::Error,
        message: "This expression is always nullish.",
    },
    DiagnosticMessage {
        code: 2872,
        category: DiagnosticCategory::Error,
        message: "This kind of expression is always truthy.",
    },
    DiagnosticMessage {
        code: 2873,
        category: DiagnosticCategory::Error,
        message: "This kind of expression is always falsy.",
    },
    DiagnosticMessage {
        code: 2874,
        category: DiagnosticCategory::Error,
        message: "This JSX tag requires '{0}' to be in scope, but it could not be found.",
    },
    DiagnosticMessage {
        code: 2875,
        category: DiagnosticCategory::Error,
        message: "This JSX tag requires the module path '{0}' to exist, but none could be found. Make sure you have types for the appropriate package installed.",
    },
    DiagnosticMessage {
        code: 2876,
        category: DiagnosticCategory::Error,
        message: "This relative import path is unsafe to rewrite because it looks like a file name, but actually resolves to \"{0}\".",
    },
    DiagnosticMessage {
        code: 2877,
        category: DiagnosticCategory::Error,
        message: "This import uses a '{0}' extension to resolve to an input TypeScript file, but will not be rewritten during emit because it is not a relative path.",
    },
    DiagnosticMessage {
        code: 2878,
        category: DiagnosticCategory::Error,
        message: "This import path is unsafe to rewrite because it resolves to another project, and the relative path between the projects' output files is not the same as the relative path between its input files.",
    },
    DiagnosticMessage {
        code: 2879,
        category: DiagnosticCategory::Error,
        message: "Using JSX fragments requires fragment factory '{0}' to be in scope, but it could not be found.",
    },
    DiagnosticMessage {
        code: 2880,
        category: DiagnosticCategory::Error,
        message: "Import assertions have been replaced by import attributes. Use 'with' instead of 'assert'.",
    },
    DiagnosticMessage {
        code: 2881,
        category: DiagnosticCategory::Error,
        message: "This expression is never nullish.",
    },
    DiagnosticMessage {
        code: 2882,
        category: DiagnosticCategory::Error,
        message: "Cannot find module or type declarations for side-effect import of '{0}'.",
    },
    DiagnosticMessage {
        code: 4000,
        category: DiagnosticCategory::Error,
        message: "Import declaration '{0}' is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4002,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4004,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4006,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of constructor signature from exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4008,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of call signature from exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4010,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of public static method from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4012,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of public method from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4014,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of method from exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4016,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of exported function has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4019,
        category: DiagnosticCategory::Error,
        message: "Implements clause of exported class '{0}' has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4020,
        category: DiagnosticCategory::Error,
        message: "'extends' clause of exported class '{0}' has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4021,
        category: DiagnosticCategory::Error,
        message: "'extends' clause of exported class has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4022,
        category: DiagnosticCategory::Error,
        message: "'extends' clause of exported interface '{0}' has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4023,
        category: DiagnosticCategory::Error,
        message: "Exported variable '{0}' has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4024,
        category: DiagnosticCategory::Error,
        message: "Exported variable '{0}' has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4025,
        category: DiagnosticCategory::Error,
        message: "Exported variable '{0}' has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4026,
        category: DiagnosticCategory::Error,
        message: "Public static property '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4027,
        category: DiagnosticCategory::Error,
        message: "Public static property '{0}' of exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4028,
        category: DiagnosticCategory::Error,
        message: "Public static property '{0}' of exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4029,
        category: DiagnosticCategory::Error,
        message: "Public property '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4030,
        category: DiagnosticCategory::Error,
        message: "Public property '{0}' of exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4031,
        category: DiagnosticCategory::Error,
        message: "Public property '{0}' of exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4032,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' of exported interface has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4033,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' of exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4034,
        category: DiagnosticCategory::Error,
        message: "Parameter type of public static setter '{0}' from exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4035,
        category: DiagnosticCategory::Error,
        message: "Parameter type of public static setter '{0}' from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4036,
        category: DiagnosticCategory::Error,
        message: "Parameter type of public setter '{0}' from exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4037,
        category: DiagnosticCategory::Error,
        message: "Parameter type of public setter '{0}' from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4038,
        category: DiagnosticCategory::Error,
        message: "Return type of public static getter '{0}' from exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4039,
        category: DiagnosticCategory::Error,
        message: "Return type of public static getter '{0}' from exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4040,
        category: DiagnosticCategory::Error,
        message: "Return type of public static getter '{0}' from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4041,
        category: DiagnosticCategory::Error,
        message: "Return type of public getter '{0}' from exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4042,
        category: DiagnosticCategory::Error,
        message: "Return type of public getter '{0}' from exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4043,
        category: DiagnosticCategory::Error,
        message: "Return type of public getter '{0}' from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4044,
        category: DiagnosticCategory::Error,
        message: "Return type of constructor signature from exported interface has or is using name '{0}' from private module '{1}'.",
    },
    DiagnosticMessage {
        code: 4045,
        category: DiagnosticCategory::Error,
        message: "Return type of constructor signature from exported interface has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4046,
        category: DiagnosticCategory::Error,
        message: "Return type of call signature from exported interface has or is using name '{0}' from private module '{1}'.",
    },
    DiagnosticMessage {
        code: 4047,
        category: DiagnosticCategory::Error,
        message: "Return type of call signature from exported interface has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4048,
        category: DiagnosticCategory::Error,
        message: "Return type of index signature from exported interface has or is using name '{0}' from private module '{1}'.",
    },
    DiagnosticMessage {
        code: 4049,
        category: DiagnosticCategory::Error,
        message: "Return type of index signature from exported interface has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4050,
        category: DiagnosticCategory::Error,
        message: "Return type of public static method from exported class has or is using name '{0}' from external module {1} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4051,
        category: DiagnosticCategory::Error,
        message: "Return type of public static method from exported class has or is using name '{0}' from private module '{1}'.",
    },
    DiagnosticMessage {
        code: 4052,
        category: DiagnosticCategory::Error,
        message: "Return type of public static method from exported class has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4053,
        category: DiagnosticCategory::Error,
        message: "Return type of public method from exported class has or is using name '{0}' from external module {1} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4054,
        category: DiagnosticCategory::Error,
        message: "Return type of public method from exported class has or is using name '{0}' from private module '{1}'.",
    },
    DiagnosticMessage {
        code: 4055,
        category: DiagnosticCategory::Error,
        message: "Return type of public method from exported class has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4056,
        category: DiagnosticCategory::Error,
        message: "Return type of method from exported interface has or is using name '{0}' from private module '{1}'.",
    },
    DiagnosticMessage {
        code: 4057,
        category: DiagnosticCategory::Error,
        message: "Return type of method from exported interface has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4058,
        category: DiagnosticCategory::Error,
        message: "Return type of exported function has or is using name '{0}' from external module {1} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4059,
        category: DiagnosticCategory::Error,
        message: "Return type of exported function has or is using name '{0}' from private module '{1}'.",
    },
    DiagnosticMessage {
        code: 4060,
        category: DiagnosticCategory::Error,
        message: "Return type of exported function has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4061,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of constructor from exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4062,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of constructor from exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4063,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of constructor from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4064,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of constructor signature from exported interface has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4065,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of constructor signature from exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4066,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of call signature from exported interface has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4067,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of call signature from exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4068,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of public static method from exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4069,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of public static method from exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4070,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of public static method from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4071,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of public method from exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4072,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of public method from exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4073,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of public method from exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4074,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of method from exported interface has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4075,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of method from exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4076,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of exported function has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4077,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of exported function has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4078,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of exported function has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4081,
        category: DiagnosticCategory::Error,
        message: "Exported type alias '{0}' has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4082,
        category: DiagnosticCategory::Error,
        message: "Default export of the module has or is using private name '{0}'.",
    },
    DiagnosticMessage {
        code: 4083,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of exported type alias has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4084,
        category: DiagnosticCategory::Error,
        message: "Exported type alias '{0}' has or is using private name '{1}' from module {2}.",
    },
    DiagnosticMessage {
        code: 4085,
        category: DiagnosticCategory::Error,
        message: "Extends clause for inferred type '{0}' has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4091,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of index signature from exported interface has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4092,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of index signature from exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4094,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' of exported anonymous class type may not be private or protected.",
    },
    DiagnosticMessage {
        code: 4095,
        category: DiagnosticCategory::Error,
        message: "Public static method '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4096,
        category: DiagnosticCategory::Error,
        message: "Public static method '{0}' of exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4097,
        category: DiagnosticCategory::Error,
        message: "Public static method '{0}' of exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4098,
        category: DiagnosticCategory::Error,
        message: "Public method '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.",
    },
    DiagnosticMessage {
        code: 4099,
        category: DiagnosticCategory::Error,
        message: "Public method '{0}' of exported class has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4100,
        category: DiagnosticCategory::Error,
        message: "Public method '{0}' of exported class has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4101,
        category: DiagnosticCategory::Error,
        message: "Method '{0}' of exported interface has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4102,
        category: DiagnosticCategory::Error,
        message: "Method '{0}' of exported interface has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4103,
        category: DiagnosticCategory::Error,
        message: "Type parameter '{0}' of exported mapped object type is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4104,
        category: DiagnosticCategory::Error,
        message: "The type '{0}' is 'readonly' and cannot be assigned to the mutable type '{1}'.",
    },
    DiagnosticMessage {
        code: 4105,
        category: DiagnosticCategory::Error,
        message: "Private or protected member '{0}' cannot be accessed on a type parameter.",
    },
    DiagnosticMessage {
        code: 4106,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of accessor has or is using private name '{1}'.",
    },
    DiagnosticMessage {
        code: 4107,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of accessor has or is using name '{1}' from private module '{2}'.",
    },
    DiagnosticMessage {
        code: 4108,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' of accessor has or is using name '{1}' from external module '{2}' but cannot be named.",
    },
    DiagnosticMessage {
        code: 4109,
        category: DiagnosticCategory::Error,
        message: "Type arguments for '{0}' circularly reference themselves.",
    },
    DiagnosticMessage {
        code: 4110,
        category: DiagnosticCategory::Error,
        message: "Tuple type arguments circularly reference themselves.",
    },
    DiagnosticMessage {
        code: 4111,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' comes from an index signature, so it must be accessed with ['{0}'].",
    },
    DiagnosticMessage {
        code: 4112,
        category: DiagnosticCategory::Error,
        message: "This member cannot have an 'override' modifier because its containing class '{0}' does not extend another class.",
    },
    DiagnosticMessage {
        code: 4113,
        category: DiagnosticCategory::Error,
        message: "This member cannot have an 'override' modifier because it is not declared in the base class '{0}'.",
    },
    DiagnosticMessage {
        code: 4114,
        category: DiagnosticCategory::Error,
        message: "This member must have an 'override' modifier because it overrides a member in the base class '{0}'.",
    },
    DiagnosticMessage {
        code: 4115,
        category: DiagnosticCategory::Error,
        message: "This parameter property must have an 'override' modifier because it overrides a member in base class '{0}'.",
    },
    DiagnosticMessage {
        code: 4116,
        category: DiagnosticCategory::Error,
        message: "This member must have an 'override' modifier because it overrides an abstract method that is declared in the base class '{0}'.",
    },
    DiagnosticMessage {
        code: 4117,
        category: DiagnosticCategory::Error,
        message: "This member cannot have an 'override' modifier because it is not declared in the base class '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 4118,
        category: DiagnosticCategory::Error,
        message: "The type of this node cannot be serialized because its property '{0}' cannot be serialized.",
    },
    DiagnosticMessage {
        code: 4119,
        category: DiagnosticCategory::Error,
        message: "This member must have a JSDoc comment with an '@override' tag because it overrides a member in the base class '{0}'.",
    },
    DiagnosticMessage {
        code: 4120,
        category: DiagnosticCategory::Error,
        message: "This parameter property must have a JSDoc comment with an '@override' tag because it overrides a member in the base class '{0}'.",
    },
    DiagnosticMessage {
        code: 4121,
        category: DiagnosticCategory::Error,
        message: "This member cannot have a JSDoc comment with an '@override' tag because its containing class '{0}' does not extend another class.",
    },
    DiagnosticMessage {
        code: 4122,
        category: DiagnosticCategory::Error,
        message: "This member cannot have a JSDoc comment with an '@override' tag because it is not declared in the base class '{0}'.",
    },
    DiagnosticMessage {
        code: 4123,
        category: DiagnosticCategory::Error,
        message: "This member cannot have a JSDoc comment with an 'override' tag because it is not declared in the base class '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 4124,
        category: DiagnosticCategory::Error,
        message: "Compiler option '{0}' of value '{1}' is unstable. Use nightly TypeScript to silence this error. Try updating with 'npm install -D typescript@next'.",
    },
    DiagnosticMessage {
        code: 4125,
        category: DiagnosticCategory::Error,
        message: "Each declaration of '{0}.{1}' differs in its value, where '{2}' was expected but '{3}' was given.",
    },
    DiagnosticMessage {
        code: 4126,
        category: DiagnosticCategory::Error,
        message: "One value of '{0}.{1}' is the string '{2}', and the other is assumed to be an unknown numeric value.",
    },
    DiagnosticMessage {
        code: 4127,
        category: DiagnosticCategory::Error,
        message: "This member cannot have an 'override' modifier because its name is dynamic.",
    },
    DiagnosticMessage {
        code: 4128,
        category: DiagnosticCategory::Error,
        message: "This member cannot have a JSDoc comment with an '@override' tag because its name is dynamic.",
    },
    DiagnosticMessage {
        code: 5001,
        category: DiagnosticCategory::Error,
        message: "The current host does not support the '{0}' option.",
    },
    DiagnosticMessage {
        code: 5009,
        category: DiagnosticCategory::Error,
        message: "Cannot find the common subdirectory path for the input files.",
    },
    DiagnosticMessage {
        code: 5010,
        category: DiagnosticCategory::Error,
        message: "File specification cannot end in a recursive directory wildcard ('**'): '{0}'.",
    },
    DiagnosticMessage {
        code: 5011,
        category: DiagnosticCategory::Error,
        message: "The common source directory of '{0}' is '{1}'. The 'rootDir' setting must be explicitly set to this or another path to adjust your output's file layout.",
    },
    DiagnosticMessage {
        code: 5012,
        category: DiagnosticCategory::Error,
        message: "Cannot read file '{0}': {1}.",
    },
    DiagnosticMessage {
        code: 5023,
        category: DiagnosticCategory::Error,
        message: "Unknown compiler option '{0}'.",
    },
    DiagnosticMessage {
        code: 5024,
        category: DiagnosticCategory::Error,
        message: "Compiler option '{0}' requires a value of type {1}.",
    },
    DiagnosticMessage {
        code: 5025,
        category: DiagnosticCategory::Error,
        message: "Unknown compiler option '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 5033,
        category: DiagnosticCategory::Error,
        message: "Could not write file '{0}': {1}.",
    },
    DiagnosticMessage {
        code: 5042,
        category: DiagnosticCategory::Error,
        message: "Option 'project' cannot be mixed with source files on a command line.",
    },
    DiagnosticMessage {
        code: 5047,
        category: DiagnosticCategory::Error,
        message: "Option 'isolatedModules' can only be used when either option '--module' is provided or option 'target' is 'ES2015' or higher.",
    },
    DiagnosticMessage {
        code: 5051,
        category: DiagnosticCategory::Error,
        message: "Option '{0} can only be used when either option '--inlineSourceMap' or option '--sourceMap' is provided.",
    },
    DiagnosticMessage {
        code: 5052,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' cannot be specified without specifying option '{1}'.",
    },
    DiagnosticMessage {
        code: 5053,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' cannot be specified with option '{1}'.",
    },
    DiagnosticMessage {
        code: 5054,
        category: DiagnosticCategory::Error,
        message: "A 'tsconfig.json' file is already defined at: '{0}'.",
    },
    DiagnosticMessage {
        code: 5055,
        category: DiagnosticCategory::Error,
        message: "Cannot write file '{0}' because it would overwrite input file.",
    },
    DiagnosticMessage {
        code: 5056,
        category: DiagnosticCategory::Error,
        message: "Cannot write file '{0}' because it would be overwritten by multiple input files.",
    },
    DiagnosticMessage {
        code: 5057,
        category: DiagnosticCategory::Error,
        message: "Cannot find a tsconfig.json file at the specified directory: '{0}'.",
    },
    DiagnosticMessage {
        code: 5058,
        category: DiagnosticCategory::Error,
        message: "The specified path does not exist: '{0}'.",
    },
    DiagnosticMessage {
        code: 5059,
        category: DiagnosticCategory::Error,
        message: "Invalid value for '--reactNamespace'. '{0}' is not a valid identifier.",
    },
    DiagnosticMessage {
        code: 5061,
        category: DiagnosticCategory::Error,
        message: "Pattern '{0}' can have at most one '*' character.",
    },
    DiagnosticMessage {
        code: 5062,
        category: DiagnosticCategory::Error,
        message: "Substitution '{0}' in pattern '{1}' can have at most one '*' character.",
    },
    DiagnosticMessage {
        code: 5063,
        category: DiagnosticCategory::Error,
        message: "Substitutions for pattern '{0}' should be an array.",
    },
    DiagnosticMessage {
        code: 5064,
        category: DiagnosticCategory::Error,
        message: "Substitution '{0}' for pattern '{1}' has incorrect type, expected 'string', got '{2}'.",
    },
    DiagnosticMessage {
        code: 5065,
        category: DiagnosticCategory::Error,
        message: "File specification cannot contain a parent directory ('..') that appears after a recursive directory wildcard ('**'): '{0}'.",
    },
    DiagnosticMessage {
        code: 5066,
        category: DiagnosticCategory::Error,
        message: "Substitutions for pattern '{0}' shouldn't be an empty array.",
    },
    DiagnosticMessage {
        code: 5067,
        category: DiagnosticCategory::Error,
        message: "Invalid value for 'jsxFactory'. '{0}' is not a valid identifier or qualified-name.",
    },
    DiagnosticMessage {
        code: 5068,
        category: DiagnosticCategory::Error,
        message: "Adding a tsconfig.json file will help organize projects that contain both TypeScript and JavaScript files. Learn more at https://aka.ms/tsconfig.",
    },
    DiagnosticMessage {
        code: 5069,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' cannot be specified without specifying option '{1}' or option '{2}'.",
    },
    DiagnosticMessage {
        code: 5070,
        category: DiagnosticCategory::Error,
        message: "Option '--resolveJsonModule' cannot be specified when 'moduleResolution' is set to 'classic'.",
    },
    DiagnosticMessage {
        code: 5071,
        category: DiagnosticCategory::Error,
        message: "Option '--resolveJsonModule' cannot be specified when 'module' is set to 'none', 'system', or 'umd'.",
    },
    DiagnosticMessage {
        code: 5072,
        category: DiagnosticCategory::Error,
        message: "Unknown build option '{0}'.",
    },
    DiagnosticMessage {
        code: 5073,
        category: DiagnosticCategory::Error,
        message: "Build option '{0}' requires a value of type {1}.",
    },
    DiagnosticMessage {
        code: 5074,
        category: DiagnosticCategory::Error,
        message: "Option '--incremental' can only be specified using tsconfig, emitting to single file or when option '--tsBuildInfoFile' is specified.",
    },
    DiagnosticMessage {
        code: 5075,
        category: DiagnosticCategory::Error,
        message: "'{0}' is assignable to the constraint of type '{1}', but '{1}' could be instantiated with a different subtype of constraint '{2}'.",
    },
    DiagnosticMessage {
        code: 5076,
        category: DiagnosticCategory::Error,
        message: "'{0}' and '{1}' operations cannot be mixed without parentheses.",
    },
    DiagnosticMessage {
        code: 5077,
        category: DiagnosticCategory::Error,
        message: "Unknown build option '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 5078,
        category: DiagnosticCategory::Error,
        message: "Unknown watch option '{0}'.",
    },
    DiagnosticMessage {
        code: 5079,
        category: DiagnosticCategory::Error,
        message: "Unknown watch option '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 5080,
        category: DiagnosticCategory::Error,
        message: "Watch option '{0}' requires a value of type {1}.",
    },
    DiagnosticMessage {
        code: 5081,
        category: DiagnosticCategory::Error,
        message: "Cannot find a tsconfig.json file at the current directory: {0}.",
    },
    DiagnosticMessage {
        code: 5082,
        category: DiagnosticCategory::Error,
        message: "'{0}' could be instantiated with an arbitrary type which could be unrelated to '{1}'.",
    },
    DiagnosticMessage {
        code: 5083,
        category: DiagnosticCategory::Error,
        message: "Cannot read file '{0}'.",
    },
    DiagnosticMessage {
        code: 5085,
        category: DiagnosticCategory::Error,
        message: "A tuple member cannot be both optional and rest.",
    },
    DiagnosticMessage {
        code: 5086,
        category: DiagnosticCategory::Error,
        message: "A labeled tuple element is declared as optional with a question mark after the name and before the colon, rather than after the type.",
    },
    DiagnosticMessage {
        code: 5087,
        category: DiagnosticCategory::Error,
        message: "A labeled tuple element is declared as rest with a '...' before the name, rather than before the type.",
    },
    DiagnosticMessage {
        code: 5088,
        category: DiagnosticCategory::Error,
        message: "The inferred type of '{0}' references a type with a cyclic structure which cannot be trivially serialized. A type annotation is necessary.",
    },
    DiagnosticMessage {
        code: 5089,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' cannot be specified when option 'jsx' is '{1}'.",
    },
    DiagnosticMessage {
        code: 5090,
        category: DiagnosticCategory::Error,
        message: "Non-relative paths are not allowed when 'baseUrl' is not set. Did you forget a leading './'?",
    },
    DiagnosticMessage {
        code: 5091,
        category: DiagnosticCategory::Error,
        message: "Option 'preserveConstEnums' cannot be disabled when '{0}' is enabled.",
    },
    DiagnosticMessage {
        code: 5092,
        category: DiagnosticCategory::Error,
        message: "The root value of a '{0}' file must be an object.",
    },
    DiagnosticMessage {
        code: 5093,
        category: DiagnosticCategory::Error,
        message: "Compiler option '--{0}' may only be used with '--build'.",
    },
    DiagnosticMessage {
        code: 5094,
        category: DiagnosticCategory::Error,
        message: "Compiler option '--{0}' may not be used with '--build'.",
    },
    DiagnosticMessage {
        code: 5095,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' can only be used when 'module' is set to 'preserve', 'commonjs', or 'es2015' or later.",
    },
    DiagnosticMessage {
        code: 5096,
        category: DiagnosticCategory::Error,
        message: "Option 'allowImportingTsExtensions' can only be used when one of 'noEmit', 'emitDeclarationOnly', or 'rewriteRelativeImportExtensions' is set.",
    },
    DiagnosticMessage {
        code: 5097,
        category: DiagnosticCategory::Error,
        message: "An import path can only end with a '{0}' extension when 'allowImportingTsExtensions' is enabled.",
    },
    DiagnosticMessage {
        code: 5098,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' can only be used when 'moduleResolution' is set to 'node16', 'nodenext', or 'bundler'.",
    },
    DiagnosticMessage {
        code: 5101,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' is deprecated and will stop functioning in TypeScript {1}. Specify compilerOption '\"ignoreDeprecations\": \"{2}\"' to silence this error.",
    },
    DiagnosticMessage {
        code: 5102,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' has been removed. Please remove it from your configuration.",
    },
    DiagnosticMessage {
        code: 5103,
        category: DiagnosticCategory::Error,
        message: "Invalid value for '--ignoreDeprecations'.",
    },
    DiagnosticMessage {
        code: 5104,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' is redundant and cannot be specified with option '{1}'.",
    },
    DiagnosticMessage {
        code: 5105,
        category: DiagnosticCategory::Error,
        message: "Option 'verbatimModuleSyntax' cannot be used when 'module' is set to 'UMD', 'AMD', or 'System'.",
    },
    DiagnosticMessage {
        code: 5106,
        category: DiagnosticCategory::Message,
        message: "Use '{0}' instead.",
    },
    DiagnosticMessage {
        code: 5107,
        category: DiagnosticCategory::Error,
        message: "Option '{0}={1}' is deprecated and will stop functioning in TypeScript {2}. Specify compilerOption '\"ignoreDeprecations\": \"{3}\"' to silence this error.",
    },
    DiagnosticMessage {
        code: 5108,
        category: DiagnosticCategory::Error,
        message: "Option '{0}={1}' has been removed. Please remove it from your configuration.",
    },
    DiagnosticMessage {
        code: 5109,
        category: DiagnosticCategory::Error,
        message: "Option 'moduleResolution' must be set to '{0}' (or left unspecified) when option 'module' is set to '{1}'.",
    },
    DiagnosticMessage {
        code: 5110,
        category: DiagnosticCategory::Error,
        message: "Option 'module' must be set to '{0}' when option 'moduleResolution' is set to '{1}'.",
    },
    DiagnosticMessage {
        code: 5111,
        category: DiagnosticCategory::Message,
        message: "Visit https://aka.ms/ts6 for migration information.",
    },
    DiagnosticMessage {
        code: 5112,
        category: DiagnosticCategory::Error,
        message: "tsconfig.json is present but will not be loaded if files are specified on commandline. Use '--ignoreConfig' to skip this error.",
    },
    DiagnosticMessage {
        code: 6000,
        category: DiagnosticCategory::Message,
        message: "Generates a sourcemap for each corresponding '.d.ts' file.",
    },
    DiagnosticMessage {
        code: 6001,
        category: DiagnosticCategory::Message,
        message: "Concatenate and emit output to single file.",
    },
    DiagnosticMessage {
        code: 6002,
        category: DiagnosticCategory::Message,
        message: "Generates corresponding '.d.ts' file.",
    },
    DiagnosticMessage {
        code: 6004,
        category: DiagnosticCategory::Message,
        message: "Specify the location where debugger should locate TypeScript files instead of source locations.",
    },
    DiagnosticMessage {
        code: 6005,
        category: DiagnosticCategory::Message,
        message: "Watch input files.",
    },
    DiagnosticMessage {
        code: 6006,
        category: DiagnosticCategory::Message,
        message: "Redirect output structure to the directory.",
    },
    DiagnosticMessage {
        code: 6007,
        category: DiagnosticCategory::Message,
        message: "Do not erase const enum declarations in generated code.",
    },
    DiagnosticMessage {
        code: 6008,
        category: DiagnosticCategory::Message,
        message: "Do not emit outputs if any errors were reported.",
    },
    DiagnosticMessage {
        code: 6009,
        category: DiagnosticCategory::Message,
        message: "Do not emit comments to output.",
    },
    DiagnosticMessage {
        code: 6010,
        category: DiagnosticCategory::Message,
        message: "Do not emit outputs.",
    },
    DiagnosticMessage {
        code: 6011,
        category: DiagnosticCategory::Message,
        message: "Allow default imports from modules with no default export. This does not affect code emit, just typechecking.",
    },
    DiagnosticMessage {
        code: 6012,
        category: DiagnosticCategory::Message,
        message: "Skip type checking of declaration files.",
    },
    DiagnosticMessage {
        code: 6013,
        category: DiagnosticCategory::Message,
        message: "Do not resolve the real path of symlinks.",
    },
    DiagnosticMessage {
        code: 6014,
        category: DiagnosticCategory::Message,
        message: "Only emit '.d.ts' declaration files.",
    },
    DiagnosticMessage {
        code: 6015,
        category: DiagnosticCategory::Message,
        message: "Specify ECMAScript target version.",
    },
    DiagnosticMessage {
        code: 6016,
        category: DiagnosticCategory::Message,
        message: "Specify module code generation.",
    },
    DiagnosticMessage {
        code: 6017,
        category: DiagnosticCategory::Message,
        message: "Print this message.",
    },
    DiagnosticMessage {
        code: 6019,
        category: DiagnosticCategory::Message,
        message: "Print the compiler's version.",
    },
    DiagnosticMessage {
        code: 6020,
        category: DiagnosticCategory::Message,
        message: "Compile the project given the path to its configuration file, or to a folder with a 'tsconfig.json'.",
    },
    DiagnosticMessage {
        code: 6023,
        category: DiagnosticCategory::Message,
        message: "Syntax: {0}",
    },
    DiagnosticMessage {
        code: 6024,
        category: DiagnosticCategory::Message,
        message: "options",
    },
    DiagnosticMessage {
        code: 6025,
        category: DiagnosticCategory::Message,
        message: "file",
    },
    DiagnosticMessage {
        code: 6026,
        category: DiagnosticCategory::Message,
        message: "Examples: {0}",
    },
    DiagnosticMessage {
        code: 6027,
        category: DiagnosticCategory::Message,
        message: "Options:",
    },
    DiagnosticMessage {
        code: 6029,
        category: DiagnosticCategory::Message,
        message: "Version {0}",
    },
    DiagnosticMessage {
        code: 6030,
        category: DiagnosticCategory::Message,
        message: "Insert command line options and files from a file.",
    },
    DiagnosticMessage {
        code: 6031,
        category: DiagnosticCategory::Message,
        message: "Starting compilation in watch mode...",
    },
    DiagnosticMessage {
        code: 6032,
        category: DiagnosticCategory::Message,
        message: "File change detected. Starting incremental compilation...",
    },
    DiagnosticMessage {
        code: 6034,
        category: DiagnosticCategory::Message,
        message: "KIND",
    },
    DiagnosticMessage {
        code: 6035,
        category: DiagnosticCategory::Message,
        message: "FILE",
    },
    DiagnosticMessage {
        code: 6036,
        category: DiagnosticCategory::Message,
        message: "VERSION",
    },
    DiagnosticMessage {
        code: 6037,
        category: DiagnosticCategory::Message,
        message: "LOCATION",
    },
    DiagnosticMessage {
        code: 6038,
        category: DiagnosticCategory::Message,
        message: "DIRECTORY",
    },
    DiagnosticMessage {
        code: 6039,
        category: DiagnosticCategory::Message,
        message: "STRATEGY",
    },
    DiagnosticMessage {
        code: 6040,
        category: DiagnosticCategory::Message,
        message: "FILE OR DIRECTORY",
    },
    DiagnosticMessage {
        code: 6041,
        category: DiagnosticCategory::Message,
        message: "Errors  Files",
    },
    DiagnosticMessage {
        code: 6043,
        category: DiagnosticCategory::Message,
        message: "Generates corresponding '.map' file.",
    },
    DiagnosticMessage {
        code: 6044,
        category: DiagnosticCategory::Error,
        message: "Compiler option '{0}' expects an argument.",
    },
    DiagnosticMessage {
        code: 6045,
        category: DiagnosticCategory::Error,
        message: "Unterminated quoted string in response file '{0}'.",
    },
    DiagnosticMessage {
        code: 6046,
        category: DiagnosticCategory::Error,
        message: "Argument for '{0}' option must be: {1}.",
    },
    DiagnosticMessage {
        code: 6048,
        category: DiagnosticCategory::Error,
        message: "Locale must be of the form <language> or <language>-<territory>. For example '{0}' or '{1}'.",
    },
    DiagnosticMessage {
        code: 6050,
        category: DiagnosticCategory::Error,
        message: "Unable to open file '{0}'.",
    },
    DiagnosticMessage {
        code: 6051,
        category: DiagnosticCategory::Error,
        message: "Corrupted locale file {0}.",
    },
    DiagnosticMessage {
        code: 6052,
        category: DiagnosticCategory::Message,
        message: "Raise error on expressions and declarations with an implied 'any' type.",
    },
    DiagnosticMessage {
        code: 6053,
        category: DiagnosticCategory::Error,
        message: "File '{0}' not found.",
    },
    DiagnosticMessage {
        code: 6054,
        category: DiagnosticCategory::Error,
        message: "File '{0}' has an unsupported extension. The only supported extensions are {1}.",
    },
    DiagnosticMessage {
        code: 6055,
        category: DiagnosticCategory::Message,
        message: "Suppress noImplicitAny errors for indexing objects lacking index signatures.",
    },
    DiagnosticMessage {
        code: 6056,
        category: DiagnosticCategory::Message,
        message: "Do not emit declarations for code that has an '@internal' annotation.",
    },
    DiagnosticMessage {
        code: 6058,
        category: DiagnosticCategory::Message,
        message: "Specify the root directory of input files. Use to control the output directory structure with --outDir.",
    },
    DiagnosticMessage {
        code: 6059,
        category: DiagnosticCategory::Error,
        message: "File '{0}' is not under 'rootDir' '{1}'. 'rootDir' is expected to contain all source files.",
    },
    DiagnosticMessage {
        code: 6060,
        category: DiagnosticCategory::Message,
        message: "Specify the end of line sequence to be used when emitting files: 'CRLF' (dos) or 'LF' (unix).",
    },
    DiagnosticMessage {
        code: 6061,
        category: DiagnosticCategory::Message,
        message: "NEWLINE",
    },
    DiagnosticMessage {
        code: 6064,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' can only be specified in 'tsconfig.json' file or set to 'null' on command line.",
    },
    DiagnosticMessage {
        code: 6065,
        category: DiagnosticCategory::Message,
        message: "Enables experimental support for ES7 decorators.",
    },
    DiagnosticMessage {
        code: 6066,
        category: DiagnosticCategory::Message,
        message: "Enables experimental support for emitting type metadata for decorators.",
    },
    DiagnosticMessage {
        code: 6070,
        category: DiagnosticCategory::Message,
        message: "Initializes a TypeScript project and creates a tsconfig.json file.",
    },
    DiagnosticMessage {
        code: 6071,
        category: DiagnosticCategory::Message,
        message: "Successfully created a tsconfig.json file.",
    },
    DiagnosticMessage {
        code: 6072,
        category: DiagnosticCategory::Message,
        message: "Suppress excess property checks for object literals.",
    },
    DiagnosticMessage {
        code: 6073,
        category: DiagnosticCategory::Message,
        message: "Stylize errors and messages using color and context (experimental).",
    },
    DiagnosticMessage {
        code: 6074,
        category: DiagnosticCategory::Message,
        message: "Do not report errors on unused labels.",
    },
    DiagnosticMessage {
        code: 6075,
        category: DiagnosticCategory::Message,
        message: "Report error when not all code paths in function return a value.",
    },
    DiagnosticMessage {
        code: 6076,
        category: DiagnosticCategory::Message,
        message: "Report errors for fallthrough cases in switch statement.",
    },
    DiagnosticMessage {
        code: 6077,
        category: DiagnosticCategory::Message,
        message: "Do not report errors on unreachable code.",
    },
    DiagnosticMessage {
        code: 6078,
        category: DiagnosticCategory::Message,
        message: "Disallow inconsistently-cased references to the same file.",
    },
    DiagnosticMessage {
        code: 6079,
        category: DiagnosticCategory::Message,
        message: "Specify library files to be included in the compilation.",
    },
    DiagnosticMessage {
        code: 6080,
        category: DiagnosticCategory::Message,
        message: "Specify JSX code generation.",
    },
    DiagnosticMessage {
        code: 6082,
        category: DiagnosticCategory::Error,
        message: "Only 'amd' and 'system' modules are supported alongside --{0}.",
    },
    DiagnosticMessage {
        code: 6083,
        category: DiagnosticCategory::Message,
        message: "Base directory to resolve non-absolute module names.",
    },
    DiagnosticMessage {
        code: 6084,
        category: DiagnosticCategory::Message,
        message: "[Deprecated] Use '--jsxFactory' instead. Specify the object invoked for createElement when targeting 'react' JSX emit",
    },
    DiagnosticMessage {
        code: 6085,
        category: DiagnosticCategory::Message,
        message: "Enable tracing of the name resolution process.",
    },
    DiagnosticMessage {
        code: 6086,
        category: DiagnosticCategory::Message,
        message: "======== Resolving module '{0}' from '{1}'. ========",
    },
    DiagnosticMessage {
        code: 6087,
        category: DiagnosticCategory::Message,
        message: "Explicitly specified module resolution kind: '{0}'.",
    },
    DiagnosticMessage {
        code: 6088,
        category: DiagnosticCategory::Message,
        message: "Module resolution kind is not specified, using '{0}'.",
    },
    DiagnosticMessage {
        code: 6089,
        category: DiagnosticCategory::Message,
        message: "======== Module name '{0}' was successfully resolved to '{1}'. ========",
    },
    DiagnosticMessage {
        code: 6090,
        category: DiagnosticCategory::Message,
        message: "======== Module name '{0}' was not resolved. ========",
    },
    DiagnosticMessage {
        code: 6091,
        category: DiagnosticCategory::Message,
        message: "'paths' option is specified, looking for a pattern to match module name '{0}'.",
    },
    DiagnosticMessage {
        code: 6092,
        category: DiagnosticCategory::Message,
        message: "Module name '{0}', matched pattern '{1}'.",
    },
    DiagnosticMessage {
        code: 6093,
        category: DiagnosticCategory::Message,
        message: "Trying substitution '{0}', candidate module location: '{1}'.",
    },
    DiagnosticMessage {
        code: 6094,
        category: DiagnosticCategory::Message,
        message: "Resolving module name '{0}' relative to base url '{1}' - '{2}'.",
    },
    DiagnosticMessage {
        code: 6095,
        category: DiagnosticCategory::Message,
        message: "Loading module as file / folder, candidate module location '{0}', target file types: {1}.",
    },
    DiagnosticMessage {
        code: 6096,
        category: DiagnosticCategory::Message,
        message: "File '{0}' does not exist.",
    },
    DiagnosticMessage {
        code: 6097,
        category: DiagnosticCategory::Message,
        message: "File '{0}' exists - use it as a name resolution result.",
    },
    DiagnosticMessage {
        code: 6098,
        category: DiagnosticCategory::Message,
        message: "Loading module '{0}' from 'node_modules' folder, target file types: {1}.",
    },
    DiagnosticMessage {
        code: 6099,
        category: DiagnosticCategory::Message,
        message: "Found 'package.json' at '{0}'.",
    },
    DiagnosticMessage {
        code: 6100,
        category: DiagnosticCategory::Message,
        message: "'package.json' does not have a '{0}' field.",
    },
    DiagnosticMessage {
        code: 6101,
        category: DiagnosticCategory::Message,
        message: "'package.json' has '{0}' field '{1}' that references '{2}'.",
    },
    DiagnosticMessage {
        code: 6102,
        category: DiagnosticCategory::Message,
        message: "Allow javascript files to be compiled.",
    },
    DiagnosticMessage {
        code: 6104,
        category: DiagnosticCategory::Message,
        message: "Checking if '{0}' is the longest matching prefix for '{1}' - '{2}'.",
    },
    DiagnosticMessage {
        code: 6105,
        category: DiagnosticCategory::Message,
        message: "Expected type of '{0}' field in 'package.json' to be '{1}', got '{2}'.",
    },
    DiagnosticMessage {
        code: 6106,
        category: DiagnosticCategory::Message,
        message: "'baseUrl' option is set to '{0}', using this value to resolve non-relative module name '{1}'.",
    },
    DiagnosticMessage {
        code: 6107,
        category: DiagnosticCategory::Message,
        message: "'rootDirs' option is set, using it to resolve relative module name '{0}'.",
    },
    DiagnosticMessage {
        code: 6108,
        category: DiagnosticCategory::Message,
        message: "Longest matching prefix for '{0}' is '{1}'.",
    },
    DiagnosticMessage {
        code: 6109,
        category: DiagnosticCategory::Message,
        message: "Loading '{0}' from the root dir '{1}', candidate location '{2}'.",
    },
    DiagnosticMessage {
        code: 6110,
        category: DiagnosticCategory::Message,
        message: "Trying other entries in 'rootDirs'.",
    },
    DiagnosticMessage {
        code: 6111,
        category: DiagnosticCategory::Message,
        message: "Module resolution using 'rootDirs' has failed.",
    },
    DiagnosticMessage {
        code: 6112,
        category: DiagnosticCategory::Message,
        message: "Do not emit 'use strict' directives in module output.",
    },
    DiagnosticMessage {
        code: 6113,
        category: DiagnosticCategory::Message,
        message: "Enable strict null checks.",
    },
    DiagnosticMessage {
        code: 6114,
        category: DiagnosticCategory::Error,
        message: "Unknown option 'excludes'. Did you mean 'exclude'?",
    },
    DiagnosticMessage {
        code: 6115,
        category: DiagnosticCategory::Message,
        message: "Raise error on 'this' expressions with an implied 'any' type.",
    },
    DiagnosticMessage {
        code: 6116,
        category: DiagnosticCategory::Message,
        message: "======== Resolving type reference directive '{0}', containing file '{1}', root directory '{2}'. ========",
    },
    DiagnosticMessage {
        code: 6119,
        category: DiagnosticCategory::Message,
        message: "======== Type reference directive '{0}' was successfully resolved to '{1}', primary: {2}. ========",
    },
    DiagnosticMessage {
        code: 6120,
        category: DiagnosticCategory::Message,
        message: "======== Type reference directive '{0}' was not resolved. ========",
    },
    DiagnosticMessage {
        code: 6121,
        category: DiagnosticCategory::Message,
        message: "Resolving with primary search path '{0}'.",
    },
    DiagnosticMessage {
        code: 6122,
        category: DiagnosticCategory::Message,
        message: "Root directory cannot be determined, skipping primary search paths.",
    },
    DiagnosticMessage {
        code: 6123,
        category: DiagnosticCategory::Message,
        message: "======== Resolving type reference directive '{0}', containing file '{1}', root directory not set. ========",
    },
    DiagnosticMessage {
        code: 6124,
        category: DiagnosticCategory::Message,
        message: "Type declaration files to be included in compilation.",
    },
    DiagnosticMessage {
        code: 6125,
        category: DiagnosticCategory::Message,
        message: "Looking up in 'node_modules' folder, initial location '{0}'.",
    },
    DiagnosticMessage {
        code: 6126,
        category: DiagnosticCategory::Message,
        message: "Containing file is not specified and root directory cannot be determined, skipping lookup in 'node_modules' folder.",
    },
    DiagnosticMessage {
        code: 6127,
        category: DiagnosticCategory::Message,
        message: "======== Resolving type reference directive '{0}', containing file not set, root directory '{1}'. ========",
    },
    DiagnosticMessage {
        code: 6128,
        category: DiagnosticCategory::Message,
        message: "======== Resolving type reference directive '{0}', containing file not set, root directory not set. ========",
    },
    DiagnosticMessage {
        code: 6130,
        category: DiagnosticCategory::Message,
        message: "Resolving real path for '{0}', result '{1}'.",
    },
    DiagnosticMessage {
        code: 6131,
        category: DiagnosticCategory::Error,
        message: "Cannot compile modules using option '{0}' unless the '--module' flag is 'amd' or 'system'.",
    },
    DiagnosticMessage {
        code: 6132,
        category: DiagnosticCategory::Message,
        message: "File name '{0}' has a '{1}' extension - stripping it.",
    },
    DiagnosticMessage {
        code: 6133,
        category: DiagnosticCategory::Error,
        message: "'{0}' is declared but its value is never read.",
    },
    DiagnosticMessage {
        code: 6134,
        category: DiagnosticCategory::Message,
        message: "Report errors on unused locals.",
    },
    DiagnosticMessage {
        code: 6135,
        category: DiagnosticCategory::Message,
        message: "Report errors on unused parameters.",
    },
    DiagnosticMessage {
        code: 6136,
        category: DiagnosticCategory::Message,
        message: "The maximum dependency depth to search under node_modules and load JavaScript files.",
    },
    DiagnosticMessage {
        code: 6137,
        category: DiagnosticCategory::Error,
        message: "Cannot import type declaration files. Consider importing '{0}' instead of '{1}'.",
    },
    DiagnosticMessage {
        code: 6138,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is declared but its value is never read.",
    },
    DiagnosticMessage {
        code: 6139,
        category: DiagnosticCategory::Message,
        message: "Import emit helpers from 'tslib'.",
    },
    DiagnosticMessage {
        code: 6140,
        category: DiagnosticCategory::Error,
        message: "Auto discovery for typings is enabled in project '{0}'. Running extra resolution pass for module '{1}' using cache location '{2}'.",
    },
    DiagnosticMessage {
        code: 6141,
        category: DiagnosticCategory::Message,
        message: "Parse in strict mode and emit \"use strict\" for each source file.",
    },
    DiagnosticMessage {
        code: 6142,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' was resolved to '{1}', but '--jsx' is not set.",
    },
    DiagnosticMessage {
        code: 6144,
        category: DiagnosticCategory::Message,
        message: "Module '{0}' was resolved as locally declared ambient module in file '{1}'.",
    },
    DiagnosticMessage {
        code: 6146,
        category: DiagnosticCategory::Message,
        message: "Specify the JSX factory function to use when targeting 'react' JSX emit, e.g. 'React.createElement' or 'h'.",
    },
    DiagnosticMessage {
        code: 6147,
        category: DiagnosticCategory::Message,
        message: "Resolution for module '{0}' was found in cache from location '{1}'.",
    },
    DiagnosticMessage {
        code: 6148,
        category: DiagnosticCategory::Message,
        message: "Directory '{0}' does not exist, skipping all lookups in it.",
    },
    DiagnosticMessage {
        code: 6149,
        category: DiagnosticCategory::Message,
        message: "Show diagnostic information.",
    },
    DiagnosticMessage {
        code: 6150,
        category: DiagnosticCategory::Message,
        message: "Show verbose diagnostic information.",
    },
    DiagnosticMessage {
        code: 6151,
        category: DiagnosticCategory::Message,
        message: "Emit a single file with source maps instead of having a separate file.",
    },
    DiagnosticMessage {
        code: 6152,
        category: DiagnosticCategory::Message,
        message: "Emit the source alongside the sourcemaps within a single file; requires '--inlineSourceMap' or '--sourceMap' to be set.",
    },
    DiagnosticMessage {
        code: 6153,
        category: DiagnosticCategory::Message,
        message: "Transpile each file as a separate module (similar to 'ts.transpileModule').",
    },
    DiagnosticMessage {
        code: 6154,
        category: DiagnosticCategory::Message,
        message: "Print names of generated files part of the compilation.",
    },
    DiagnosticMessage {
        code: 6155,
        category: DiagnosticCategory::Message,
        message: "Print names of files part of the compilation.",
    },
    DiagnosticMessage {
        code: 6156,
        category: DiagnosticCategory::Message,
        message: "The locale used when displaying messages to the user (e.g. 'en-us')",
    },
    DiagnosticMessage {
        code: 6157,
        category: DiagnosticCategory::Message,
        message: "Do not generate custom helper functions like '__extends' in compiled output.",
    },
    DiagnosticMessage {
        code: 6158,
        category: DiagnosticCategory::Message,
        message: "Do not include the default library file (lib.d.ts).",
    },
    DiagnosticMessage {
        code: 6159,
        category: DiagnosticCategory::Message,
        message: "Do not add triple-slash references or imported modules to the list of compiled files.",
    },
    DiagnosticMessage {
        code: 6160,
        category: DiagnosticCategory::Message,
        message: "[Deprecated] Use '--skipLibCheck' instead. Skip type checking of default library declaration files.",
    },
    DiagnosticMessage {
        code: 6161,
        category: DiagnosticCategory::Message,
        message: "List of folders to include type definitions from.",
    },
    DiagnosticMessage {
        code: 6162,
        category: DiagnosticCategory::Message,
        message: "Disable size limitations on JavaScript projects.",
    },
    DiagnosticMessage {
        code: 6163,
        category: DiagnosticCategory::Message,
        message: "The character set of the input files.",
    },
    DiagnosticMessage {
        code: 6164,
        category: DiagnosticCategory::Message,
        message: "Skipping module '{0}' that looks like an absolute URI, target file types: {1}.",
    },
    DiagnosticMessage {
        code: 6165,
        category: DiagnosticCategory::Message,
        message: "Do not truncate error messages.",
    },
    DiagnosticMessage {
        code: 6166,
        category: DiagnosticCategory::Message,
        message: "Output directory for generated declaration files.",
    },
    DiagnosticMessage {
        code: 6167,
        category: DiagnosticCategory::Message,
        message: "A series of entries which re-map imports to lookup locations relative to the 'baseUrl'.",
    },
    DiagnosticMessage {
        code: 6168,
        category: DiagnosticCategory::Message,
        message: "List of root folders whose combined content represents the structure of the project at runtime.",
    },
    DiagnosticMessage {
        code: 6169,
        category: DiagnosticCategory::Message,
        message: "Show all compiler options.",
    },
    DiagnosticMessage {
        code: 6170,
        category: DiagnosticCategory::Message,
        message: "[Deprecated] Use '--outFile' instead. Concatenate and emit output to single file",
    },
    DiagnosticMessage {
        code: 6171,
        category: DiagnosticCategory::Message,
        message: "Command-line Options",
    },
    DiagnosticMessage {
        code: 6179,
        category: DiagnosticCategory::Message,
        message: "Provide full support for iterables in 'for-of', spread, and destructuring when targeting 'ES5'.",
    },
    DiagnosticMessage {
        code: 6180,
        category: DiagnosticCategory::Message,
        message: "Enable all strict type-checking options.",
    },
    DiagnosticMessage {
        code: 6182,
        category: DiagnosticCategory::Message,
        message: "Scoped package detected, looking in '{0}'",
    },
    DiagnosticMessage {
        code: 6183,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of module '{0}' from '{1}' of old program, it was successfully resolved to '{2}'.",
    },
    DiagnosticMessage {
        code: 6184,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of module '{0}' from '{1}' of old program, it was successfully resolved to '{2}' with Package ID '{3}'.",
    },
    DiagnosticMessage {
        code: 6186,
        category: DiagnosticCategory::Message,
        message: "Enable strict checking of function types.",
    },
    DiagnosticMessage {
        code: 6187,
        category: DiagnosticCategory::Message,
        message: "Enable strict checking of property initialization in classes.",
    },
    DiagnosticMessage {
        code: 6188,
        category: DiagnosticCategory::Error,
        message: "Numeric separators are not allowed here.",
    },
    DiagnosticMessage {
        code: 6189,
        category: DiagnosticCategory::Error,
        message: "Multiple consecutive numeric separators are not permitted.",
    },
    DiagnosticMessage {
        code: 6191,
        category: DiagnosticCategory::Message,
        message: "Whether to keep outdated console output in watch mode instead of clearing the screen.",
    },
    DiagnosticMessage {
        code: 6192,
        category: DiagnosticCategory::Error,
        message: "All imports in import declaration are unused.",
    },
    DiagnosticMessage {
        code: 6193,
        category: DiagnosticCategory::Message,
        message: "Found 1 error. Watching for file changes.",
    },
    DiagnosticMessage {
        code: 6194,
        category: DiagnosticCategory::Message,
        message: "Found {0} errors. Watching for file changes.",
    },
    DiagnosticMessage {
        code: 6195,
        category: DiagnosticCategory::Message,
        message: "Resolve 'keyof' to string valued property names only (no numbers or symbols).",
    },
    DiagnosticMessage {
        code: 6196,
        category: DiagnosticCategory::Error,
        message: "'{0}' is declared but never used.",
    },
    DiagnosticMessage {
        code: 6197,
        category: DiagnosticCategory::Message,
        message: "Include modules imported with '.json' extension",
    },
    DiagnosticMessage {
        code: 6198,
        category: DiagnosticCategory::Error,
        message: "All destructured elements are unused.",
    },
    DiagnosticMessage {
        code: 6199,
        category: DiagnosticCategory::Error,
        message: "All variables are unused.",
    },
    DiagnosticMessage {
        code: 6200,
        category: DiagnosticCategory::Error,
        message: "Definitions of the following identifiers conflict with those in another file: {0}",
    },
    DiagnosticMessage {
        code: 6201,
        category: DiagnosticCategory::Message,
        message: "Conflicts are in this file.",
    },
    DiagnosticMessage {
        code: 6202,
        category: DiagnosticCategory::Error,
        message: "Project references may not form a circular graph. Cycle detected: {0}",
    },
    DiagnosticMessage {
        code: 6203,
        category: DiagnosticCategory::Message,
        message: "'{0}' was also declared here.",
    },
    DiagnosticMessage {
        code: 6204,
        category: DiagnosticCategory::Message,
        message: "and here.",
    },
    DiagnosticMessage {
        code: 6205,
        category: DiagnosticCategory::Error,
        message: "All type parameters are unused.",
    },
    DiagnosticMessage {
        code: 6206,
        category: DiagnosticCategory::Message,
        message: "'package.json' has a 'typesVersions' field with version-specific path mappings.",
    },
    DiagnosticMessage {
        code: 6207,
        category: DiagnosticCategory::Message,
        message: "'package.json' does not have a 'typesVersions' entry that matches version '{0}'.",
    },
    DiagnosticMessage {
        code: 6208,
        category: DiagnosticCategory::Message,
        message: "'package.json' has a 'typesVersions' entry '{0}' that matches compiler version '{1}', looking for a pattern to match module name '{2}'.",
    },
    DiagnosticMessage {
        code: 6209,
        category: DiagnosticCategory::Message,
        message: "'package.json' has a 'typesVersions' entry '{0}' that is not a valid semver range.",
    },
    DiagnosticMessage {
        code: 6210,
        category: DiagnosticCategory::Message,
        message: "An argument for '{0}' was not provided.",
    },
    DiagnosticMessage {
        code: 6211,
        category: DiagnosticCategory::Message,
        message: "An argument matching this binding pattern was not provided.",
    },
    DiagnosticMessage {
        code: 6212,
        category: DiagnosticCategory::Message,
        message: "Did you mean to call this expression?",
    },
    DiagnosticMessage {
        code: 6213,
        category: DiagnosticCategory::Message,
        message: "Did you mean to use 'new' with this expression?",
    },
    DiagnosticMessage {
        code: 6214,
        category: DiagnosticCategory::Message,
        message: "Enable strict 'bind', 'call', and 'apply' methods on functions.",
    },
    DiagnosticMessage {
        code: 6215,
        category: DiagnosticCategory::Message,
        message: "Using compiler options of project reference redirect '{0}'.",
    },
    DiagnosticMessage {
        code: 6216,
        category: DiagnosticCategory::Message,
        message: "Found 1 error.",
    },
    DiagnosticMessage {
        code: 6217,
        category: DiagnosticCategory::Message,
        message: "Found {0} errors.",
    },
    DiagnosticMessage {
        code: 6218,
        category: DiagnosticCategory::Message,
        message: "======== Module name '{0}' was successfully resolved to '{1}' with Package ID '{2}'. ========",
    },
    DiagnosticMessage {
        code: 6219,
        category: DiagnosticCategory::Message,
        message: "======== Type reference directive '{0}' was successfully resolved to '{1}' with Package ID '{2}', primary: {3}. ========",
    },
    DiagnosticMessage {
        code: 6220,
        category: DiagnosticCategory::Message,
        message: "'package.json' had a falsy '{0}' field.",
    },
    DiagnosticMessage {
        code: 6221,
        category: DiagnosticCategory::Message,
        message: "Disable use of source files instead of declaration files from referenced projects.",
    },
    DiagnosticMessage {
        code: 6222,
        category: DiagnosticCategory::Message,
        message: "Emit class fields with Define instead of Set.",
    },
    DiagnosticMessage {
        code: 6223,
        category: DiagnosticCategory::Message,
        message: "Generates a CPU profile.",
    },
    DiagnosticMessage {
        code: 6224,
        category: DiagnosticCategory::Message,
        message: "Disable solution searching for this project.",
    },
    DiagnosticMessage {
        code: 6225,
        category: DiagnosticCategory::Message,
        message: "Specify strategy for watching file: 'FixedPollingInterval' (default), 'PriorityPollingInterval', 'DynamicPriorityPolling', 'FixedChunkSizePolling', 'UseFsEvents', 'UseFsEventsOnParentDirectory'.",
    },
    DiagnosticMessage {
        code: 6226,
        category: DiagnosticCategory::Message,
        message: "Specify strategy for watching directory on platforms that don't support recursive watching natively: 'UseFsEvents' (default), 'FixedPollingInterval', 'DynamicPriorityPolling', 'FixedChunkSizePolling'.",
    },
    DiagnosticMessage {
        code: 6227,
        category: DiagnosticCategory::Message,
        message: "Specify strategy for creating a polling watch when it fails to create using file system events: 'FixedInterval' (default), 'PriorityInterval', 'DynamicPriority', 'FixedChunkSize'.",
    },
    DiagnosticMessage {
        code: 6229,
        category: DiagnosticCategory::Error,
        message: "Tag '{0}' expects at least '{1}' arguments, but the JSX factory '{2}' provides at most '{3}'.",
    },
    DiagnosticMessage {
        code: 6230,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' can only be specified in 'tsconfig.json' file or set to 'false' or 'null' on command line.",
    },
    DiagnosticMessage {
        code: 6231,
        category: DiagnosticCategory::Error,
        message: "Could not resolve the path '{0}' with the extensions: {1}.",
    },
    DiagnosticMessage {
        code: 6232,
        category: DiagnosticCategory::Error,
        message: "Declaration augments declaration in another file. This cannot be serialized.",
    },
    DiagnosticMessage {
        code: 6233,
        category: DiagnosticCategory::Error,
        message: "This is the declaration being augmented. Consider moving the augmenting declaration into the same file.",
    },
    DiagnosticMessage {
        code: 6234,
        category: DiagnosticCategory::Error,
        message: "This expression is not callable because it is a 'get' accessor. Did you mean to use it without '()'?",
    },
    DiagnosticMessage {
        code: 6235,
        category: DiagnosticCategory::Message,
        message: "Disable loading referenced projects.",
    },
    DiagnosticMessage {
        code: 6236,
        category: DiagnosticCategory::Error,
        message: "Arguments for the rest parameter '{0}' were not provided.",
    },
    DiagnosticMessage {
        code: 6237,
        category: DiagnosticCategory::Message,
        message: "Generates an event trace and a list of types.",
    },
    DiagnosticMessage {
        code: 6238,
        category: DiagnosticCategory::Error,
        message: "Specify the module specifier to be used to import the 'jsx' and 'jsxs' factory functions from. eg, react",
    },
    DiagnosticMessage {
        code: 6239,
        category: DiagnosticCategory::Message,
        message: "File '{0}' exists according to earlier cached lookups.",
    },
    DiagnosticMessage {
        code: 6240,
        category: DiagnosticCategory::Message,
        message: "File '{0}' does not exist according to earlier cached lookups.",
    },
    DiagnosticMessage {
        code: 6241,
        category: DiagnosticCategory::Message,
        message: "Resolution for type reference directive '{0}' was found in cache from location '{1}'.",
    },
    DiagnosticMessage {
        code: 6242,
        category: DiagnosticCategory::Message,
        message: "======== Resolving type reference directive '{0}', containing file '{1}'. ========",
    },
    DiagnosticMessage {
        code: 6243,
        category: DiagnosticCategory::Message,
        message: "Interpret optional property types as written, rather than adding 'undefined'.",
    },
    DiagnosticMessage {
        code: 6244,
        category: DiagnosticCategory::Message,
        message: "Modules",
    },
    DiagnosticMessage {
        code: 6245,
        category: DiagnosticCategory::Message,
        message: "File Management",
    },
    DiagnosticMessage {
        code: 6246,
        category: DiagnosticCategory::Message,
        message: "Emit",
    },
    DiagnosticMessage {
        code: 6247,
        category: DiagnosticCategory::Message,
        message: "JavaScript Support",
    },
    DiagnosticMessage {
        code: 6248,
        category: DiagnosticCategory::Message,
        message: "Type Checking",
    },
    DiagnosticMessage {
        code: 6249,
        category: DiagnosticCategory::Message,
        message: "Editor Support",
    },
    DiagnosticMessage {
        code: 6250,
        category: DiagnosticCategory::Message,
        message: "Watch and Build Modes",
    },
    DiagnosticMessage {
        code: 6251,
        category: DiagnosticCategory::Message,
        message: "Compiler Diagnostics",
    },
    DiagnosticMessage {
        code: 6252,
        category: DiagnosticCategory::Message,
        message: "Interop Constraints",
    },
    DiagnosticMessage {
        code: 6253,
        category: DiagnosticCategory::Message,
        message: "Backwards Compatibility",
    },
    DiagnosticMessage {
        code: 6254,
        category: DiagnosticCategory::Message,
        message: "Language and Environment",
    },
    DiagnosticMessage {
        code: 6255,
        category: DiagnosticCategory::Message,
        message: "Projects",
    },
    DiagnosticMessage {
        code: 6256,
        category: DiagnosticCategory::Message,
        message: "Output Formatting",
    },
    DiagnosticMessage {
        code: 6257,
        category: DiagnosticCategory::Message,
        message: "Completeness",
    },
    DiagnosticMessage {
        code: 6258,
        category: DiagnosticCategory::Error,
        message: "'{0}' should be set inside the 'compilerOptions' object of the config json file",
    },
    DiagnosticMessage {
        code: 6259,
        category: DiagnosticCategory::Message,
        message: "Found 1 error in {0}",
    },
    DiagnosticMessage {
        code: 6260,
        category: DiagnosticCategory::Message,
        message: "Found {0} errors in the same file, starting at: {1}",
    },
    DiagnosticMessage {
        code: 6261,
        category: DiagnosticCategory::Message,
        message: "Found {0} errors in {1} files.",
    },
    DiagnosticMessage {
        code: 6262,
        category: DiagnosticCategory::Message,
        message: "File name '{0}' has a '{1}' extension - looking up '{2}' instead.",
    },
    DiagnosticMessage {
        code: 6263,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' was resolved to '{1}', but '--allowArbitraryExtensions' is not set.",
    },
    DiagnosticMessage {
        code: 6264,
        category: DiagnosticCategory::Message,
        message: "Enable importing files with any extension, provided a declaration file is present.",
    },
    DiagnosticMessage {
        code: 6265,
        category: DiagnosticCategory::Message,
        message: "Resolving type reference directive for program that specifies custom typeRoots, skipping lookup in 'node_modules' folder.",
    },
    DiagnosticMessage {
        code: 6266,
        category: DiagnosticCategory::Error,
        message: "Option '{0}' can only be specified on command line.",
    },
    DiagnosticMessage {
        code: 6270,
        category: DiagnosticCategory::Message,
        message: "Directory '{0}' has no containing package.json scope. Imports will not resolve.",
    },
    DiagnosticMessage {
        code: 6271,
        category: DiagnosticCategory::Message,
        message: "Import specifier '{0}' does not exist in package.json scope at path '{1}'.",
    },
    DiagnosticMessage {
        code: 6272,
        category: DiagnosticCategory::Message,
        message: "Invalid import specifier '{0}' has no possible resolutions.",
    },
    DiagnosticMessage {
        code: 6273,
        category: DiagnosticCategory::Message,
        message: "package.json scope '{0}' has no imports defined.",
    },
    DiagnosticMessage {
        code: 6274,
        category: DiagnosticCategory::Message,
        message: "package.json scope '{0}' explicitly maps specifier '{1}' to null.",
    },
    DiagnosticMessage {
        code: 6275,
        category: DiagnosticCategory::Message,
        message: "package.json scope '{0}' has invalid type for target of specifier '{1}'",
    },
    DiagnosticMessage {
        code: 6276,
        category: DiagnosticCategory::Message,
        message: "Export specifier '{0}' does not exist in package.json scope at path '{1}'.",
    },
    DiagnosticMessage {
        code: 6277,
        category: DiagnosticCategory::Message,
        message: "Resolution of non-relative name failed; trying with modern Node resolution features disabled to see if npm library needs configuration update.",
    },
    DiagnosticMessage {
        code: 6278,
        category: DiagnosticCategory::Message,
        message: "There are types at '{0}', but this result could not be resolved when respecting package.json \"exports\". The '{1}' library may need to update its package.json or typings.",
    },
    DiagnosticMessage {
        code: 6279,
        category: DiagnosticCategory::Message,
        message: "Resolution of non-relative name failed; trying with '--moduleResolution bundler' to see if project may need configuration update.",
    },
    DiagnosticMessage {
        code: 6280,
        category: DiagnosticCategory::Message,
        message: "There are types at '{0}', but this result could not be resolved under your current 'moduleResolution' setting. Consider updating to 'node16', 'nodenext', or 'bundler'.",
    },
    DiagnosticMessage {
        code: 6281,
        category: DiagnosticCategory::Message,
        message: "'package.json' has a 'peerDependencies' field.",
    },
    DiagnosticMessage {
        code: 6282,
        category: DiagnosticCategory::Message,
        message: "Found peerDependency '{0}' with '{1}' version.",
    },
    DiagnosticMessage {
        code: 6283,
        category: DiagnosticCategory::Message,
        message: "Failed to find peerDependency '{0}'.",
    },
    DiagnosticMessage {
        code: 6284,
        category: DiagnosticCategory::Message,
        message: "File Layout",
    },
    DiagnosticMessage {
        code: 6285,
        category: DiagnosticCategory::Message,
        message: "Environment Settings",
    },
    DiagnosticMessage {
        code: 6286,
        category: DiagnosticCategory::Message,
        message: "See also https://aka.ms/tsconfig/module",
    },
    DiagnosticMessage {
        code: 6287,
        category: DiagnosticCategory::Message,
        message: "For nodejs:",
    },
    DiagnosticMessage {
        code: 6290,
        category: DiagnosticCategory::Message,
        message: "and npm install -D @types/node",
    },
    DiagnosticMessage {
        code: 6291,
        category: DiagnosticCategory::Message,
        message: "Other Outputs",
    },
    DiagnosticMessage {
        code: 6292,
        category: DiagnosticCategory::Message,
        message: "Stricter Typechecking Options",
    },
    DiagnosticMessage {
        code: 6293,
        category: DiagnosticCategory::Message,
        message: "Style Options",
    },
    DiagnosticMessage {
        code: 6294,
        category: DiagnosticCategory::Message,
        message: "Recommended Options",
    },
    DiagnosticMessage {
        code: 6302,
        category: DiagnosticCategory::Message,
        message: "Enable project compilation",
    },
    DiagnosticMessage {
        code: 6304,
        category: DiagnosticCategory::Error,
        message: "Composite projects may not disable declaration emit.",
    },
    DiagnosticMessage {
        code: 6305,
        category: DiagnosticCategory::Error,
        message: "Output file '{0}' has not been built from source file '{1}'.",
    },
    DiagnosticMessage {
        code: 6306,
        category: DiagnosticCategory::Error,
        message: "Referenced project '{0}' must have setting \"composite\": true.",
    },
    DiagnosticMessage {
        code: 6307,
        category: DiagnosticCategory::Error,
        message: "File '{0}' is not listed within the file list of project '{1}'. Projects must list all files or use an 'include' pattern.",
    },
    DiagnosticMessage {
        code: 6310,
        category: DiagnosticCategory::Error,
        message: "Referenced project '{0}' may not disable emit.",
    },
    DiagnosticMessage {
        code: 6350,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because output '{1}' is older than input '{2}'",
    },
    DiagnosticMessage {
        code: 6351,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is up to date because newest input '{1}' is older than output '{2}'",
    },
    DiagnosticMessage {
        code: 6352,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because output file '{1}' does not exist",
    },
    DiagnosticMessage {
        code: 6353,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because its dependency '{1}' is out of date",
    },
    DiagnosticMessage {
        code: 6354,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is up to date with .d.ts files from its dependencies",
    },
    DiagnosticMessage {
        code: 6355,
        category: DiagnosticCategory::Message,
        message: "Projects in this build: {0}",
    },
    DiagnosticMessage {
        code: 6356,
        category: DiagnosticCategory::Message,
        message: "A non-dry build would delete the following files: {0}",
    },
    DiagnosticMessage {
        code: 6357,
        category: DiagnosticCategory::Message,
        message: "A non-dry build would build project '{0}'",
    },
    DiagnosticMessage {
        code: 6358,
        category: DiagnosticCategory::Message,
        message: "Building project '{0}'...",
    },
    DiagnosticMessage {
        code: 6359,
        category: DiagnosticCategory::Message,
        message: "Updating output timestamps of project '{0}'...",
    },
    DiagnosticMessage {
        code: 6361,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is up to date",
    },
    DiagnosticMessage {
        code: 6362,
        category: DiagnosticCategory::Message,
        message: "Skipping build of project '{0}' because its dependency '{1}' has errors",
    },
    DiagnosticMessage {
        code: 6363,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' can't be built because its dependency '{1}' has errors",
    },
    DiagnosticMessage {
        code: 6364,
        category: DiagnosticCategory::Message,
        message: "Build one or more projects and their dependencies, if out of date",
    },
    DiagnosticMessage {
        code: 6365,
        category: DiagnosticCategory::Message,
        message: "Delete the outputs of all projects.",
    },
    DiagnosticMessage {
        code: 6367,
        category: DiagnosticCategory::Message,
        message: "Show what would be built (or deleted, if specified with '--clean')",
    },
    DiagnosticMessage {
        code: 6369,
        category: DiagnosticCategory::Error,
        message: "Option '--build' must be the first command line argument.",
    },
    DiagnosticMessage {
        code: 6370,
        category: DiagnosticCategory::Error,
        message: "Options '{0}' and '{1}' cannot be combined.",
    },
    DiagnosticMessage {
        code: 6371,
        category: DiagnosticCategory::Message,
        message: "Updating unchanged output timestamps of project '{0}'...",
    },
    DiagnosticMessage {
        code: 6374,
        category: DiagnosticCategory::Message,
        message: "A non-dry build would update timestamps for output of project '{0}'",
    },
    DiagnosticMessage {
        code: 6377,
        category: DiagnosticCategory::Error,
        message: "Cannot write file '{0}' because it will overwrite '.tsbuildinfo' file generated by referenced project '{1}'",
    },
    DiagnosticMessage {
        code: 6379,
        category: DiagnosticCategory::Error,
        message: "Composite projects may not disable incremental compilation.",
    },
    DiagnosticMessage {
        code: 6380,
        category: DiagnosticCategory::Message,
        message: "Specify file to store incremental compilation information",
    },
    DiagnosticMessage {
        code: 6381,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because output for it was generated with version '{1}' that differs with current version '{2}'",
    },
    DiagnosticMessage {
        code: 6382,
        category: DiagnosticCategory::Message,
        message: "Skipping build of project '{0}' because its dependency '{1}' was not built",
    },
    DiagnosticMessage {
        code: 6383,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' can't be built because its dependency '{1}' was not built",
    },
    DiagnosticMessage {
        code: 6384,
        category: DiagnosticCategory::Message,
        message: "Have recompiles in '--incremental' and '--watch' assume that changes within a file will only affect files directly depending on it.",
    },
    DiagnosticMessage {
        code: 6385,
        category: DiagnosticCategory::Suggestion,
        message: "'{0}' is deprecated.",
    },
    DiagnosticMessage {
        code: 6386,
        category: DiagnosticCategory::Message,
        message: "Performance timings for '--diagnostics' or '--extendedDiagnostics' are not available in this session. A native implementation of the Web Performance API could not be found.",
    },
    DiagnosticMessage {
        code: 6387,
        category: DiagnosticCategory::Suggestion,
        message: "The signature '{0}' of '{1}' is deprecated.",
    },
    DiagnosticMessage {
        code: 6388,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is being forcibly rebuilt",
    },
    DiagnosticMessage {
        code: 6389,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of module '{0}' from '{1}' of old program, it was not resolved.",
    },
    DiagnosticMessage {
        code: 6390,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of type reference directive '{0}' from '{1}' of old program, it was successfully resolved to '{2}'.",
    },
    DiagnosticMessage {
        code: 6391,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of type reference directive '{0}' from '{1}' of old program, it was successfully resolved to '{2}' with Package ID '{3}'.",
    },
    DiagnosticMessage {
        code: 6392,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of type reference directive '{0}' from '{1}' of old program, it was not resolved.",
    },
    DiagnosticMessage {
        code: 6393,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of module '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}'.",
    },
    DiagnosticMessage {
        code: 6394,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of module '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}' with Package ID '{4}'.",
    },
    DiagnosticMessage {
        code: 6395,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of module '{0}' from '{1}' found in cache from location '{2}', it was not resolved.",
    },
    DiagnosticMessage {
        code: 6396,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of type reference directive '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}'.",
    },
    DiagnosticMessage {
        code: 6397,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of type reference directive '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}' with Package ID '{4}'.",
    },
    DiagnosticMessage {
        code: 6398,
        category: DiagnosticCategory::Message,
        message: "Reusing resolution of type reference directive '{0}' from '{1}' found in cache from location '{2}', it was not resolved.",
    },
    DiagnosticMessage {
        code: 6399,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because buildinfo file '{1}' indicates that some of the changes were not emitted",
    },
    DiagnosticMessage {
        code: 6400,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is up to date but needs to update timestamps of output files that are older than input files",
    },
    DiagnosticMessage {
        code: 6401,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because there was error reading file '{1}'",
    },
    DiagnosticMessage {
        code: 6402,
        category: DiagnosticCategory::Message,
        message: "Resolving in {0} mode with conditions {1}.",
    },
    DiagnosticMessage {
        code: 6403,
        category: DiagnosticCategory::Message,
        message: "Matched '{0}' condition '{1}'.",
    },
    DiagnosticMessage {
        code: 6404,
        category: DiagnosticCategory::Message,
        message: "Using '{0}' subpath '{1}' with target '{2}'.",
    },
    DiagnosticMessage {
        code: 6405,
        category: DiagnosticCategory::Message,
        message: "Saw non-matching condition '{0}'.",
    },
    DiagnosticMessage {
        code: 6406,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because buildinfo file '{1}' indicates there is change in compilerOptions",
    },
    DiagnosticMessage {
        code: 6407,
        category: DiagnosticCategory::Message,
        message: "Allow imports to include TypeScript file extensions. Requires '--moduleResolution bundler' and either '--noEmit' or '--emitDeclarationOnly' to be set.",
    },
    DiagnosticMessage {
        code: 6408,
        category: DiagnosticCategory::Message,
        message: "Use the package.json 'exports' field when resolving package imports.",
    },
    DiagnosticMessage {
        code: 6409,
        category: DiagnosticCategory::Message,
        message: "Use the package.json 'imports' field when resolving imports.",
    },
    DiagnosticMessage {
        code: 6410,
        category: DiagnosticCategory::Message,
        message: "Conditions to set in addition to the resolver-specific defaults when resolving imports.",
    },
    DiagnosticMessage {
        code: 6411,
        category: DiagnosticCategory::Message,
        message: "`true` when 'moduleResolution' is 'node16', 'nodenext', or 'bundler'; otherwise `false`.",
    },
    DiagnosticMessage {
        code: 6412,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because buildinfo file '{1}' indicates that file '{2}' was root file of compilation but not any more.",
    },
    DiagnosticMessage {
        code: 6413,
        category: DiagnosticCategory::Message,
        message: "Entering conditional exports.",
    },
    DiagnosticMessage {
        code: 6414,
        category: DiagnosticCategory::Message,
        message: "Resolved under condition '{0}'.",
    },
    DiagnosticMessage {
        code: 6415,
        category: DiagnosticCategory::Message,
        message: "Failed to resolve under condition '{0}'.",
    },
    DiagnosticMessage {
        code: 6416,
        category: DiagnosticCategory::Message,
        message: "Exiting conditional exports.",
    },
    DiagnosticMessage {
        code: 6417,
        category: DiagnosticCategory::Message,
        message: "Searching all ancestor node_modules directories for preferred extensions: {0}.",
    },
    DiagnosticMessage {
        code: 6418,
        category: DiagnosticCategory::Message,
        message: "Searching all ancestor node_modules directories for fallback extensions: {0}.",
    },
    DiagnosticMessage {
        code: 6419,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because buildinfo file '{1}' indicates that program needs to report errors.",
    },
    DiagnosticMessage {
        code: 6420,
        category: DiagnosticCategory::Message,
        message: "Project '{0}' is out of date because {1}.",
    },
    DiagnosticMessage {
        code: 6421,
        category: DiagnosticCategory::Message,
        message: "Rewrite '.ts', '.tsx', '.mts', and '.cts' file extensions in relative import paths to their JavaScript equivalent in output files.",
    },
    DiagnosticMessage {
        code: 6500,
        category: DiagnosticCategory::Message,
        message: "The expected type comes from property '{0}' which is declared here on type '{1}'",
    },
    DiagnosticMessage {
        code: 6501,
        category: DiagnosticCategory::Message,
        message: "The expected type comes from this index signature.",
    },
    DiagnosticMessage {
        code: 6502,
        category: DiagnosticCategory::Message,
        message: "The expected type comes from the return type of this signature.",
    },
    DiagnosticMessage {
        code: 6503,
        category: DiagnosticCategory::Message,
        message: "Print names of files that are part of the compilation and then stop processing.",
    },
    DiagnosticMessage {
        code: 6504,
        category: DiagnosticCategory::Error,
        message: "File '{0}' is a JavaScript file. Did you mean to enable the 'allowJs' option?",
    },
    DiagnosticMessage {
        code: 6505,
        category: DiagnosticCategory::Message,
        message: "Print names of files and the reason they are part of the compilation.",
    },
    DiagnosticMessage {
        code: 6506,
        category: DiagnosticCategory::Message,
        message: "Consider adding a 'declare' modifier to this class.",
    },
    DiagnosticMessage {
        code: 6600,
        category: DiagnosticCategory::Message,
        message: "Allow JavaScript files to be a part of your program. Use the 'checkJs' option to get errors from these files.",
    },
    DiagnosticMessage {
        code: 6601,
        category: DiagnosticCategory::Message,
        message: "Allow 'import x from y' when a module doesn't have a default export.",
    },
    DiagnosticMessage {
        code: 6602,
        category: DiagnosticCategory::Message,
        message: "Allow accessing UMD globals from modules.",
    },
    DiagnosticMessage {
        code: 6603,
        category: DiagnosticCategory::Message,
        message: "Disable error reporting for unreachable code.",
    },
    DiagnosticMessage {
        code: 6604,
        category: DiagnosticCategory::Message,
        message: "Disable error reporting for unused labels.",
    },
    DiagnosticMessage {
        code: 6605,
        category: DiagnosticCategory::Message,
        message: "Ensure 'use strict' is always emitted.",
    },
    DiagnosticMessage {
        code: 6606,
        category: DiagnosticCategory::Message,
        message: "Have recompiles in projects that use 'incremental' and 'watch' mode assume that changes within a file will only affect files directly depending on it.",
    },
    DiagnosticMessage {
        code: 6607,
        category: DiagnosticCategory::Message,
        message: "Specify the base directory to resolve non-relative module names.",
    },
    DiagnosticMessage {
        code: 6608,
        category: DiagnosticCategory::Message,
        message: "No longer supported. In early versions, manually set the text encoding for reading files.",
    },
    DiagnosticMessage {
        code: 6609,
        category: DiagnosticCategory::Message,
        message: "Enable error reporting in type-checked JavaScript files.",
    },
    DiagnosticMessage {
        code: 6611,
        category: DiagnosticCategory::Message,
        message: "Enable constraints that allow a TypeScript project to be used with project references.",
    },
    DiagnosticMessage {
        code: 6612,
        category: DiagnosticCategory::Message,
        message: "Generate .d.ts files from TypeScript and JavaScript files in your project.",
    },
    DiagnosticMessage {
        code: 6613,
        category: DiagnosticCategory::Message,
        message: "Specify the output directory for generated declaration files.",
    },
    DiagnosticMessage {
        code: 6614,
        category: DiagnosticCategory::Message,
        message: "Create sourcemaps for d.ts files.",
    },
    DiagnosticMessage {
        code: 6615,
        category: DiagnosticCategory::Message,
        message: "Output compiler performance information after building.",
    },
    DiagnosticMessage {
        code: 6616,
        category: DiagnosticCategory::Message,
        message: "Disables inference for type acquisition by looking at filenames in a project.",
    },
    DiagnosticMessage {
        code: 6617,
        category: DiagnosticCategory::Message,
        message: "Reduce the number of projects loaded automatically by TypeScript.",
    },
    DiagnosticMessage {
        code: 6618,
        category: DiagnosticCategory::Message,
        message: "Remove the 20mb cap on total source code size for JavaScript files in the TypeScript language server.",
    },
    DiagnosticMessage {
        code: 6619,
        category: DiagnosticCategory::Message,
        message: "Opt a project out of multi-project reference checking when editing.",
    },
    DiagnosticMessage {
        code: 6620,
        category: DiagnosticCategory::Message,
        message: "Disable preferring source files instead of declaration files when referencing composite projects.",
    },
    DiagnosticMessage {
        code: 6621,
        category: DiagnosticCategory::Message,
        message: "Emit more compliant, but verbose and less performant JavaScript for iteration.",
    },
    DiagnosticMessage {
        code: 6622,
        category: DiagnosticCategory::Message,
        message: "Emit a UTF-8 Byte Order Mark (BOM) in the beginning of output files.",
    },
    DiagnosticMessage {
        code: 6623,
        category: DiagnosticCategory::Message,
        message: "Only output d.ts files and not JavaScript files.",
    },
    DiagnosticMessage {
        code: 6624,
        category: DiagnosticCategory::Message,
        message: "Emit design-type metadata for decorated declarations in source files.",
    },
    DiagnosticMessage {
        code: 6625,
        category: DiagnosticCategory::Message,
        message: "Disable the type acquisition for JavaScript projects",
    },
    DiagnosticMessage {
        code: 6626,
        category: DiagnosticCategory::Message,
        message: "Emit additional JavaScript to ease support for importing CommonJS modules. This enables 'allowSyntheticDefaultImports' for type compatibility.",
    },
    DiagnosticMessage {
        code: 6627,
        category: DiagnosticCategory::Message,
        message: "Filters results from the `include` option.",
    },
    DiagnosticMessage {
        code: 6628,
        category: DiagnosticCategory::Message,
        message: "Remove a list of directories from the watch process.",
    },
    DiagnosticMessage {
        code: 6629,
        category: DiagnosticCategory::Message,
        message: "Remove a list of files from the watch mode's processing.",
    },
    DiagnosticMessage {
        code: 6630,
        category: DiagnosticCategory::Message,
        message: "Enable experimental support for legacy experimental decorators.",
    },
    DiagnosticMessage {
        code: 6631,
        category: DiagnosticCategory::Message,
        message: "Print files read during the compilation including why it was included.",
    },
    DiagnosticMessage {
        code: 6632,
        category: DiagnosticCategory::Message,
        message: "Output more detailed compiler performance information after building.",
    },
    DiagnosticMessage {
        code: 6633,
        category: DiagnosticCategory::Message,
        message: "Specify one or more path or node module references to base configuration files from which settings are inherited.",
    },
    DiagnosticMessage {
        code: 6634,
        category: DiagnosticCategory::Message,
        message: "Specify what approach the watcher should use if the system runs out of native file watchers.",
    },
    DiagnosticMessage {
        code: 6635,
        category: DiagnosticCategory::Message,
        message: "Include a list of files. This does not support glob patterns, as opposed to `include`.",
    },
    DiagnosticMessage {
        code: 6636,
        category: DiagnosticCategory::Message,
        message: "Build all projects, including those that appear to be up to date.",
    },
    DiagnosticMessage {
        code: 6637,
        category: DiagnosticCategory::Message,
        message: "Ensure that casing is correct in imports.",
    },
    DiagnosticMessage {
        code: 6638,
        category: DiagnosticCategory::Message,
        message: "Emit a v8 CPU profile of the compiler run for debugging.",
    },
    DiagnosticMessage {
        code: 6639,
        category: DiagnosticCategory::Message,
        message: "Allow importing helper functions from tslib once per project, instead of including them per-file.",
    },
    DiagnosticMessage {
        code: 6640,
        category: DiagnosticCategory::Message,
        message: "Skip building downstream projects on error in upstream project.",
    },
    DiagnosticMessage {
        code: 6641,
        category: DiagnosticCategory::Message,
        message: "Specify a list of glob patterns that match files to be included in compilation.",
    },
    DiagnosticMessage {
        code: 6642,
        category: DiagnosticCategory::Message,
        message: "Save .tsbuildinfo files to allow for incremental compilation of projects.",
    },
    DiagnosticMessage {
        code: 6643,
        category: DiagnosticCategory::Message,
        message: "Include sourcemap files inside the emitted JavaScript.",
    },
    DiagnosticMessage {
        code: 6644,
        category: DiagnosticCategory::Message,
        message: "Include source code in the sourcemaps inside the emitted JavaScript.",
    },
    DiagnosticMessage {
        code: 6645,
        category: DiagnosticCategory::Message,
        message: "Ensure that each file can be safely transpiled without relying on other imports.",
    },
    DiagnosticMessage {
        code: 6646,
        category: DiagnosticCategory::Message,
        message: "Specify what JSX code is generated.",
    },
    DiagnosticMessage {
        code: 6647,
        category: DiagnosticCategory::Message,
        message: "Specify the JSX factory function used when targeting React JSX emit, e.g. 'React.createElement' or 'h'.",
    },
    DiagnosticMessage {
        code: 6648,
        category: DiagnosticCategory::Message,
        message: "Specify the JSX Fragment reference used for fragments when targeting React JSX emit e.g. 'React.Fragment' or 'Fragment'.",
    },
    DiagnosticMessage {
        code: 6649,
        category: DiagnosticCategory::Message,
        message: "Specify module specifier used to import the JSX factory functions when using 'jsx: react-jsx*'.",
    },
    DiagnosticMessage {
        code: 6650,
        category: DiagnosticCategory::Message,
        message: "Make keyof only return strings instead of string, numbers or symbols. Legacy option.",
    },
    DiagnosticMessage {
        code: 6651,
        category: DiagnosticCategory::Message,
        message: "Specify a set of bundled library declaration files that describe the target runtime environment.",
    },
    DiagnosticMessage {
        code: 6652,
        category: DiagnosticCategory::Message,
        message: "Print the names of emitted files after a compilation.",
    },
    DiagnosticMessage {
        code: 6653,
        category: DiagnosticCategory::Message,
        message: "Print all of the files read during the compilation.",
    },
    DiagnosticMessage {
        code: 6654,
        category: DiagnosticCategory::Message,
        message: "Set the language of the messaging from TypeScript. This does not affect emit.",
    },
    DiagnosticMessage {
        code: 6655,
        category: DiagnosticCategory::Message,
        message: "Specify the location where debugger should locate map files instead of generated locations.",
    },
    DiagnosticMessage {
        code: 6656,
        category: DiagnosticCategory::Message,
        message: "Specify the maximum folder depth used for checking JavaScript files from 'node_modules'. Only applicable with 'allowJs'.",
    },
    DiagnosticMessage {
        code: 6657,
        category: DiagnosticCategory::Message,
        message: "Specify what module code is generated.",
    },
    DiagnosticMessage {
        code: 6658,
        category: DiagnosticCategory::Message,
        message: "Specify how TypeScript looks up a file from a given module specifier.",
    },
    DiagnosticMessage {
        code: 6659,
        category: DiagnosticCategory::Message,
        message: "Set the newline character for emitting files.",
    },
    DiagnosticMessage {
        code: 6660,
        category: DiagnosticCategory::Message,
        message: "Disable emitting files from a compilation.",
    },
    DiagnosticMessage {
        code: 6661,
        category: DiagnosticCategory::Message,
        message: "Disable generating custom helper functions like '__extends' in compiled output.",
    },
    DiagnosticMessage {
        code: 6662,
        category: DiagnosticCategory::Message,
        message: "Disable emitting files if any type checking errors are reported.",
    },
    DiagnosticMessage {
        code: 6663,
        category: DiagnosticCategory::Message,
        message: "Disable truncating types in error messages.",
    },
    DiagnosticMessage {
        code: 6664,
        category: DiagnosticCategory::Message,
        message: "Enable error reporting for fallthrough cases in switch statements.",
    },
    DiagnosticMessage {
        code: 6665,
        category: DiagnosticCategory::Message,
        message: "Enable error reporting for expressions and declarations with an implied 'any' type.",
    },
    DiagnosticMessage {
        code: 6666,
        category: DiagnosticCategory::Message,
        message: "Ensure overriding members in derived classes are marked with an override modifier.",
    },
    DiagnosticMessage {
        code: 6667,
        category: DiagnosticCategory::Message,
        message: "Enable error reporting for codepaths that do not explicitly return in a function.",
    },
    DiagnosticMessage {
        code: 6668,
        category: DiagnosticCategory::Message,
        message: "Enable error reporting when 'this' is given the type 'any'.",
    },
    DiagnosticMessage {
        code: 6669,
        category: DiagnosticCategory::Message,
        message: "Disable adding 'use strict' directives in emitted JavaScript files.",
    },
    DiagnosticMessage {
        code: 6670,
        category: DiagnosticCategory::Message,
        message: "Disable including any library files, including the default lib.d.ts.",
    },
    DiagnosticMessage {
        code: 6671,
        category: DiagnosticCategory::Message,
        message: "Enforces using indexed accessors for keys declared using an indexed type.",
    },
    DiagnosticMessage {
        code: 6672,
        category: DiagnosticCategory::Message,
        message: "Disallow 'import's, 'require's or '<reference>'s from expanding the number of files TypeScript should add to a project.",
    },
    DiagnosticMessage {
        code: 6673,
        category: DiagnosticCategory::Message,
        message: "Disable strict checking of generic signatures in function types.",
    },
    DiagnosticMessage {
        code: 6674,
        category: DiagnosticCategory::Message,
        message: "Add 'undefined' to a type when accessed using an index.",
    },
    DiagnosticMessage {
        code: 6675,
        category: DiagnosticCategory::Message,
        message: "Enable error reporting when local variables aren't read.",
    },
    DiagnosticMessage {
        code: 6676,
        category: DiagnosticCategory::Message,
        message: "Raise an error when a function parameter isn't read.",
    },
    DiagnosticMessage {
        code: 6677,
        category: DiagnosticCategory::Message,
        message: "Deprecated setting. Use 'outFile' instead.",
    },
    DiagnosticMessage {
        code: 6678,
        category: DiagnosticCategory::Message,
        message: "Specify an output folder for all emitted files.",
    },
    DiagnosticMessage {
        code: 6679,
        category: DiagnosticCategory::Message,
        message: "Specify a file that bundles all outputs into one JavaScript file. If 'declaration' is true, also designates a file that bundles all .d.ts output.",
    },
    DiagnosticMessage {
        code: 6680,
        category: DiagnosticCategory::Message,
        message: "Specify a set of entries that re-map imports to additional lookup locations.",
    },
    DiagnosticMessage {
        code: 6681,
        category: DiagnosticCategory::Message,
        message: "Specify a list of language service plugins to include.",
    },
    DiagnosticMessage {
        code: 6682,
        category: DiagnosticCategory::Message,
        message: "Disable erasing 'const enum' declarations in generated code.",
    },
    DiagnosticMessage {
        code: 6683,
        category: DiagnosticCategory::Message,
        message: "Disable resolving symlinks to their realpath. This correlates to the same flag in node.",
    },
    DiagnosticMessage {
        code: 6684,
        category: DiagnosticCategory::Message,
        message: "Disable wiping the console in watch mode.",
    },
    DiagnosticMessage {
        code: 6685,
        category: DiagnosticCategory::Message,
        message: "Enable color and formatting in TypeScript's output to make compiler errors easier to read.",
    },
    DiagnosticMessage {
        code: 6686,
        category: DiagnosticCategory::Message,
        message: "Specify the object invoked for 'createElement'. This only applies when targeting 'react' JSX emit.",
    },
    DiagnosticMessage {
        code: 6687,
        category: DiagnosticCategory::Message,
        message: "Specify an array of objects that specify paths for projects. Used in project references.",
    },
    DiagnosticMessage {
        code: 6688,
        category: DiagnosticCategory::Message,
        message: "Disable emitting comments.",
    },
    DiagnosticMessage {
        code: 6689,
        category: DiagnosticCategory::Message,
        message: "Enable importing .json files.",
    },
    DiagnosticMessage {
        code: 6690,
        category: DiagnosticCategory::Message,
        message: "Specify the root folder within your source files.",
    },
    DiagnosticMessage {
        code: 6691,
        category: DiagnosticCategory::Message,
        message: "Allow multiple folders to be treated as one when resolving modules.",
    },
    DiagnosticMessage {
        code: 6692,
        category: DiagnosticCategory::Message,
        message: "Skip type checking .d.ts files that are included with TypeScript.",
    },
    DiagnosticMessage {
        code: 6693,
        category: DiagnosticCategory::Message,
        message: "Skip type checking all .d.ts files.",
    },
    DiagnosticMessage {
        code: 6694,
        category: DiagnosticCategory::Message,
        message: "Create source map files for emitted JavaScript files.",
    },
    DiagnosticMessage {
        code: 6695,
        category: DiagnosticCategory::Message,
        message: "Specify the root path for debuggers to find the reference source code.",
    },
    DiagnosticMessage {
        code: 6697,
        category: DiagnosticCategory::Message,
        message: "Check that the arguments for 'bind', 'call', and 'apply' methods match the original function.",
    },
    DiagnosticMessage {
        code: 6698,
        category: DiagnosticCategory::Message,
        message: "When assigning functions, check to ensure parameters and the return values are subtype-compatible.",
    },
    DiagnosticMessage {
        code: 6699,
        category: DiagnosticCategory::Message,
        message: "When type checking, take into account 'null' and 'undefined'.",
    },
    DiagnosticMessage {
        code: 6700,
        category: DiagnosticCategory::Message,
        message: "Check for class properties that are declared but not set in the constructor.",
    },
    DiagnosticMessage {
        code: 6701,
        category: DiagnosticCategory::Message,
        message: "Disable emitting declarations that have '@internal' in their JSDoc comments.",
    },
    DiagnosticMessage {
        code: 6702,
        category: DiagnosticCategory::Message,
        message: "Disable reporting of excess property errors during the creation of object literals.",
    },
    DiagnosticMessage {
        code: 6703,
        category: DiagnosticCategory::Message,
        message: "Suppress 'noImplicitAny' errors when indexing objects that lack index signatures.",
    },
    DiagnosticMessage {
        code: 6704,
        category: DiagnosticCategory::Message,
        message: "Synchronously call callbacks and update the state of directory watchers on platforms that don`t support recursive watching natively.",
    },
    DiagnosticMessage {
        code: 6705,
        category: DiagnosticCategory::Message,
        message: "Set the JavaScript language version for emitted JavaScript and include compatible library declarations.",
    },
    DiagnosticMessage {
        code: 6706,
        category: DiagnosticCategory::Message,
        message: "Log paths used during the 'moduleResolution' process.",
    },
    DiagnosticMessage {
        code: 6707,
        category: DiagnosticCategory::Message,
        message: "Specify the path to .tsbuildinfo incremental compilation file.",
    },
    DiagnosticMessage {
        code: 6709,
        category: DiagnosticCategory::Message,
        message: "Specify options for automatic acquisition of declaration files.",
    },
    DiagnosticMessage {
        code: 6710,
        category: DiagnosticCategory::Message,
        message: "Specify multiple folders that act like './node_modules/@types'.",
    },
    DiagnosticMessage {
        code: 6711,
        category: DiagnosticCategory::Message,
        message: "Specify type package names to be included without being referenced in a source file.",
    },
    DiagnosticMessage {
        code: 6712,
        category: DiagnosticCategory::Message,
        message: "Emit ECMAScript-standard-compliant class fields.",
    },
    DiagnosticMessage {
        code: 6713,
        category: DiagnosticCategory::Message,
        message: "Enable verbose logging.",
    },
    DiagnosticMessage {
        code: 6714,
        category: DiagnosticCategory::Message,
        message: "Specify how directories are watched on systems that lack recursive file-watching functionality.",
    },
    DiagnosticMessage {
        code: 6715,
        category: DiagnosticCategory::Message,
        message: "Specify how the TypeScript watch mode works.",
    },
    DiagnosticMessage {
        code: 6717,
        category: DiagnosticCategory::Message,
        message: "Require undeclared properties from index signatures to use element accesses.",
    },
    DiagnosticMessage {
        code: 6718,
        category: DiagnosticCategory::Message,
        message: "Specify emit/checking behavior for imports that are only used for types.",
    },
    DiagnosticMessage {
        code: 6719,
        category: DiagnosticCategory::Message,
        message: "Require sufficient annotation on exports so other tools can trivially generate declaration files.",
    },
    DiagnosticMessage {
        code: 6720,
        category: DiagnosticCategory::Message,
        message: "Built-in iterators are instantiated with a 'TReturn' type of 'undefined' instead of 'any'.",
    },
    DiagnosticMessage {
        code: 6721,
        category: DiagnosticCategory::Message,
        message: "Do not allow runtime constructs that are not part of ECMAScript.",
    },
    DiagnosticMessage {
        code: 6803,
        category: DiagnosticCategory::Message,
        message: "Default catch clause variables as 'unknown' instead of 'any'.",
    },
    DiagnosticMessage {
        code: 6804,
        category: DiagnosticCategory::Message,
        message: "Do not transform or elide any imports or exports not marked as type-only, ensuring they are written in the output file's format based on the 'module' setting.",
    },
    DiagnosticMessage {
        code: 6805,
        category: DiagnosticCategory::Message,
        message: "Disable full type checking (only critical parse and emit errors will be reported).",
    },
    DiagnosticMessage {
        code: 6806,
        category: DiagnosticCategory::Message,
        message: "Check side effect imports.",
    },
    DiagnosticMessage {
        code: 6807,
        category: DiagnosticCategory::Error,
        message: "This operation can be simplified. This shift is identical to `{0} {1} {2}`.",
    },
    DiagnosticMessage {
        code: 6808,
        category: DiagnosticCategory::Message,
        message: "Enable lib replacement.",
    },
    DiagnosticMessage {
        code: 6809,
        category: DiagnosticCategory::Message,
        message: "Ensure types are ordered stably and deterministically across compilations.",
    },
    DiagnosticMessage {
        code: 6900,
        category: DiagnosticCategory::Message,
        message: "one of:",
    },
    DiagnosticMessage {
        code: 6901,
        category: DiagnosticCategory::Message,
        message: "one or more:",
    },
    DiagnosticMessage {
        code: 6902,
        category: DiagnosticCategory::Message,
        message: "type:",
    },
    DiagnosticMessage {
        code: 6903,
        category: DiagnosticCategory::Message,
        message: "default:",
    },
    DiagnosticMessage {
        code: 6905,
        category: DiagnosticCategory::Message,
        message: "`true`, unless `strict` is `false`",
    },
    DiagnosticMessage {
        code: 6906,
        category: DiagnosticCategory::Message,
        message: "`false`, unless `composite` is set",
    },
    DiagnosticMessage {
        code: 6907,
        category: DiagnosticCategory::Message,
        message: "`[\"node_modules\", \"bower_components\", \"jspm_packages\"]`, plus the value of `outDir` if one is specified.",
    },
    DiagnosticMessage {
        code: 6908,
        category: DiagnosticCategory::Message,
        message: "`[]` if `files` is specified, otherwise `[\"**/*\"]`",
    },
    DiagnosticMessage {
        code: 6909,
        category: DiagnosticCategory::Message,
        message: "`true` if `composite`, `false` otherwise",
    },
    DiagnosticMessage {
        code: 6911,
        category: DiagnosticCategory::Message,
        message: "Computed from the list of input files",
    },
    DiagnosticMessage {
        code: 6912,
        category: DiagnosticCategory::Message,
        message: "Platform specific",
    },
    DiagnosticMessage {
        code: 6913,
        category: DiagnosticCategory::Message,
        message: "You can learn about all of the compiler options at {0}",
    },
    DiagnosticMessage {
        code: 6914,
        category: DiagnosticCategory::Message,
        message: "Including --watch, -w will start watching the current project for the file changes. Once set, you can config watch mode with:",
    },
    DiagnosticMessage {
        code: 6915,
        category: DiagnosticCategory::Message,
        message: "Using --build, -b will make tsc behave more like a build orchestrator than a compiler. This is used to trigger building composite projects which you can learn more about at {0}",
    },
    DiagnosticMessage {
        code: 6916,
        category: DiagnosticCategory::Message,
        message: "COMMON COMMANDS",
    },
    DiagnosticMessage {
        code: 6917,
        category: DiagnosticCategory::Message,
        message: "ALL COMPILER OPTIONS",
    },
    DiagnosticMessage {
        code: 6918,
        category: DiagnosticCategory::Message,
        message: "WATCH OPTIONS",
    },
    DiagnosticMessage {
        code: 6919,
        category: DiagnosticCategory::Message,
        message: "BUILD OPTIONS",
    },
    DiagnosticMessage {
        code: 6920,
        category: DiagnosticCategory::Message,
        message: "COMMON COMPILER OPTIONS",
    },
    DiagnosticMessage {
        code: 6921,
        category: DiagnosticCategory::Message,
        message: "COMMAND LINE FLAGS",
    },
    DiagnosticMessage {
        code: 6922,
        category: DiagnosticCategory::Message,
        message: "tsc: The TypeScript Compiler",
    },
    DiagnosticMessage {
        code: 6923,
        category: DiagnosticCategory::Message,
        message: "Compiles the current project (tsconfig.json in the working directory.)",
    },
    DiagnosticMessage {
        code: 6924,
        category: DiagnosticCategory::Message,
        message: "Ignoring tsconfig.json, compiles the specified files with default compiler options.",
    },
    DiagnosticMessage {
        code: 6925,
        category: DiagnosticCategory::Message,
        message: "Build a composite project in the working directory.",
    },
    DiagnosticMessage {
        code: 6926,
        category: DiagnosticCategory::Message,
        message: "Creates a tsconfig.json with the recommended settings in the working directory.",
    },
    DiagnosticMessage {
        code: 6927,
        category: DiagnosticCategory::Message,
        message: "Compiles the TypeScript project located at the specified path.",
    },
    DiagnosticMessage {
        code: 6928,
        category: DiagnosticCategory::Message,
        message: "An expanded version of this information, showing all possible compiler options",
    },
    DiagnosticMessage {
        code: 6929,
        category: DiagnosticCategory::Message,
        message: "Compiles the current project, with additional settings.",
    },
    DiagnosticMessage {
        code: 6930,
        category: DiagnosticCategory::Message,
        message: "`true` for ES2022 and above, including ESNext.",
    },
    DiagnosticMessage {
        code: 6931,
        category: DiagnosticCategory::Error,
        message: "List of file name suffixes to search when resolving a module.",
    },
    DiagnosticMessage {
        code: 6932,
        category: DiagnosticCategory::Message,
        message: "`false`, unless `checkJs` is set",
    },
    DiagnosticMessage {
        code: 7005,
        category: DiagnosticCategory::Error,
        message: "Variable '{0}' implicitly has an '{1}' type.",
    },
    DiagnosticMessage {
        code: 7006,
        category: DiagnosticCategory::Error,
        message: "Parameter '{0}' implicitly has an '{1}' type.",
    },
    DiagnosticMessage {
        code: 7008,
        category: DiagnosticCategory::Error,
        message: "Member '{0}' implicitly has an '{1}' type.",
    },
    DiagnosticMessage {
        code: 7009,
        category: DiagnosticCategory::Error,
        message: "'new' expression, whose target lacks a construct signature, implicitly has an 'any' type.",
    },
    DiagnosticMessage {
        code: 7010,
        category: DiagnosticCategory::Error,
        message: "'{0}', which lacks return-type annotation, implicitly has an '{1}' return type.",
    },
    DiagnosticMessage {
        code: 7011,
        category: DiagnosticCategory::Error,
        message: "Function expression, which lacks return-type annotation, implicitly has an '{0}' return type.",
    },
    DiagnosticMessage {
        code: 7012,
        category: DiagnosticCategory::Error,
        message: "This overload implicitly returns the type '{0}' because it lacks a return type annotation.",
    },
    DiagnosticMessage {
        code: 7013,
        category: DiagnosticCategory::Error,
        message: "Construct signature, which lacks return-type annotation, implicitly has an 'any' return type.",
    },
    DiagnosticMessage {
        code: 7014,
        category: DiagnosticCategory::Error,
        message: "Function type, which lacks return-type annotation, implicitly has an '{0}' return type.",
    },
    DiagnosticMessage {
        code: 7015,
        category: DiagnosticCategory::Error,
        message: "Element implicitly has an 'any' type because index expression is not of type 'number'.",
    },
    DiagnosticMessage {
        code: 7016,
        category: DiagnosticCategory::Error,
        message: "Could not find a declaration file for module '{0}'. '{1}' implicitly has an 'any' type.",
    },
    DiagnosticMessage {
        code: 7017,
        category: DiagnosticCategory::Error,
        message: "Element implicitly has an 'any' type because type '{0}' has no index signature.",
    },
    DiagnosticMessage {
        code: 7018,
        category: DiagnosticCategory::Error,
        message: "Object literal's property '{0}' implicitly has an '{1}' type.",
    },
    DiagnosticMessage {
        code: 7019,
        category: DiagnosticCategory::Error,
        message: "Rest parameter '{0}' implicitly has an 'any[]' type.",
    },
    DiagnosticMessage {
        code: 7020,
        category: DiagnosticCategory::Error,
        message: "Call signature, which lacks return-type annotation, implicitly has an 'any' return type.",
    },
    DiagnosticMessage {
        code: 7022,
        category: DiagnosticCategory::Error,
        message: "'{0}' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.",
    },
    DiagnosticMessage {
        code: 7023,
        category: DiagnosticCategory::Error,
        message: "'{0}' implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.",
    },
    DiagnosticMessage {
        code: 7024,
        category: DiagnosticCategory::Error,
        message: "Function implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.",
    },
    DiagnosticMessage {
        code: 7025,
        category: DiagnosticCategory::Error,
        message: "Generator implicitly has yield type '{0}'. Consider supplying a return type annotation.",
    },
    DiagnosticMessage {
        code: 7026,
        category: DiagnosticCategory::Error,
        message: "JSX element implicitly has type 'any' because no interface 'JSX.{0}' exists.",
    },
    DiagnosticMessage {
        code: 7027,
        category: DiagnosticCategory::Error,
        message: "Unreachable code detected.",
    },
    DiagnosticMessage {
        code: 7028,
        category: DiagnosticCategory::Error,
        message: "Unused label.",
    },
    DiagnosticMessage {
        code: 7029,
        category: DiagnosticCategory::Error,
        message: "Fallthrough case in switch.",
    },
    DiagnosticMessage {
        code: 7030,
        category: DiagnosticCategory::Error,
        message: "Not all code paths return a value.",
    },
    DiagnosticMessage {
        code: 7031,
        category: DiagnosticCategory::Error,
        message: "Binding element '{0}' implicitly has an '{1}' type.",
    },
    DiagnosticMessage {
        code: 7032,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' implicitly has type 'any', because its set accessor lacks a parameter type annotation.",
    },
    DiagnosticMessage {
        code: 7033,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' implicitly has type 'any', because its get accessor lacks a return type annotation.",
    },
    DiagnosticMessage {
        code: 7034,
        category: DiagnosticCategory::Error,
        message: "Variable '{0}' implicitly has type '{1}' in some locations where its type cannot be determined.",
    },
    DiagnosticMessage {
        code: 7035,
        category: DiagnosticCategory::Error,
        message: "Try `npm i --save-dev @types/{1}` if it exists or add a new declaration (.d.ts) file containing `declare module '{0}';`",
    },
    DiagnosticMessage {
        code: 7036,
        category: DiagnosticCategory::Error,
        message: "Dynamic import's specifier must be of type 'string', but here has type '{0}'.",
    },
    DiagnosticMessage {
        code: 7037,
        category: DiagnosticCategory::Message,
        message: "Enables emit interoperability between CommonJS and ES Modules via creation of namespace objects for all imports. Implies 'allowSyntheticDefaultImports'.",
    },
    DiagnosticMessage {
        code: 7038,
        category: DiagnosticCategory::Message,
        message: "Type originates at this import. A namespace-style import cannot be called or constructed, and will cause a failure at runtime. Consider using a default import or import require here instead.",
    },
    DiagnosticMessage {
        code: 7039,
        category: DiagnosticCategory::Error,
        message: "Mapped object type implicitly has an 'any' template type.",
    },
    DiagnosticMessage {
        code: 7040,
        category: DiagnosticCategory::Error,
        message: "If the '{0}' package actually exposes this module, consider sending a pull request to amend 'https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/{1}'",
    },
    DiagnosticMessage {
        code: 7041,
        category: DiagnosticCategory::Error,
        message: "The containing arrow function captures the global value of 'this'.",
    },
    DiagnosticMessage {
        code: 7042,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' was resolved to '{1}', but '--resolveJsonModule' is not used.",
    },
    DiagnosticMessage {
        code: 7043,
        category: DiagnosticCategory::Suggestion,
        message: "Variable '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7044,
        category: DiagnosticCategory::Suggestion,
        message: "Parameter '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7045,
        category: DiagnosticCategory::Suggestion,
        message: "Member '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7046,
        category: DiagnosticCategory::Suggestion,
        message: "Variable '{0}' implicitly has type '{1}' in some locations, but a better type may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7047,
        category: DiagnosticCategory::Suggestion,
        message: "Rest parameter '{0}' implicitly has an 'any[]' type, but a better type may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7048,
        category: DiagnosticCategory::Suggestion,
        message: "Property '{0}' implicitly has type 'any', but a better type for its get accessor may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7049,
        category: DiagnosticCategory::Suggestion,
        message: "Property '{0}' implicitly has type 'any', but a better type for its set accessor may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7050,
        category: DiagnosticCategory::Suggestion,
        message: "'{0}' implicitly has an '{1}' return type, but a better type may be inferred from usage.",
    },
    DiagnosticMessage {
        code: 7051,
        category: DiagnosticCategory::Error,
        message: "Parameter has a name but no type. Did you mean '{0}: {1}'?",
    },
    DiagnosticMessage {
        code: 7052,
        category: DiagnosticCategory::Error,
        message: "Element implicitly has an 'any' type because type '{0}' has no index signature. Did you mean to call '{1}'?",
    },
    DiagnosticMessage {
        code: 7053,
        category: DiagnosticCategory::Error,
        message: "Element implicitly has an 'any' type because expression of type '{0}' can't be used to index type '{1}'.",
    },
    DiagnosticMessage {
        code: 7054,
        category: DiagnosticCategory::Error,
        message: "No index signature with a parameter of type '{0}' was found on type '{1}'.",
    },
    DiagnosticMessage {
        code: 7055,
        category: DiagnosticCategory::Error,
        message: "'{0}', which lacks return-type annotation, implicitly has an '{1}' yield type.",
    },
    DiagnosticMessage {
        code: 7056,
        category: DiagnosticCategory::Error,
        message: "The inferred type of this node exceeds the maximum length the compiler will serialize. An explicit type annotation is needed.",
    },
    DiagnosticMessage {
        code: 7057,
        category: DiagnosticCategory::Error,
        message: "'yield' expression implicitly results in an 'any' type because its containing generator lacks a return-type annotation.",
    },
    DiagnosticMessage {
        code: 7058,
        category: DiagnosticCategory::Error,
        message: "If the '{0}' package actually exposes this module, try adding a new declaration (.d.ts) file containing `declare module '{1}';`",
    },
    DiagnosticMessage {
        code: 7059,
        category: DiagnosticCategory::Error,
        message: "This syntax is reserved in files with the .mts or .cts extension. Use an `as` expression instead.",
    },
    DiagnosticMessage {
        code: 7060,
        category: DiagnosticCategory::Error,
        message: "This syntax is reserved in files with the .mts or .cts extension. Add a trailing comma or explicit constraint.",
    },
    DiagnosticMessage {
        code: 7061,
        category: DiagnosticCategory::Error,
        message: "A mapped type may not declare properties or methods.",
    },
    DiagnosticMessage {
        code: 8000,
        category: DiagnosticCategory::Error,
        message: "You cannot rename this element.",
    },
    DiagnosticMessage {
        code: 8001,
        category: DiagnosticCategory::Error,
        message: "You cannot rename elements that are defined in the standard TypeScript library.",
    },
    DiagnosticMessage {
        code: 8002,
        category: DiagnosticCategory::Error,
        message: "'import ... =' can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8003,
        category: DiagnosticCategory::Error,
        message: "'export =' can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8004,
        category: DiagnosticCategory::Error,
        message: "Type parameter declarations can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8005,
        category: DiagnosticCategory::Error,
        message: "'implements' clauses can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8006,
        category: DiagnosticCategory::Error,
        message: "'{0}' declarations can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8008,
        category: DiagnosticCategory::Error,
        message: "Type aliases can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8009,
        category: DiagnosticCategory::Error,
        message: "The '{0}' modifier can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8010,
        category: DiagnosticCategory::Error,
        message: "Type annotations can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8011,
        category: DiagnosticCategory::Error,
        message: "Type arguments can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8012,
        category: DiagnosticCategory::Error,
        message: "Parameter modifiers can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8013,
        category: DiagnosticCategory::Error,
        message: "Non-null assertions can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8016,
        category: DiagnosticCategory::Error,
        message: "Type assertion expressions can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8017,
        category: DiagnosticCategory::Error,
        message: "Signature declarations can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8019,
        category: DiagnosticCategory::Message,
        message: "Report errors in .js files.",
    },
    DiagnosticMessage {
        code: 8020,
        category: DiagnosticCategory::Error,
        message: "JSDoc types can only be used inside documentation comments.",
    },
    DiagnosticMessage {
        code: 8021,
        category: DiagnosticCategory::Error,
        message: "JSDoc '@typedef' tag should either have a type annotation or be followed by '@property' or '@member' tags.",
    },
    DiagnosticMessage {
        code: 8022,
        category: DiagnosticCategory::Error,
        message: "JSDoc '@{0}' is not attached to a class.",
    },
    DiagnosticMessage {
        code: 8023,
        category: DiagnosticCategory::Error,
        message: "JSDoc '@{0} {1}' does not match the 'extends {2}' clause.",
    },
    DiagnosticMessage {
        code: 8024,
        category: DiagnosticCategory::Error,
        message: "JSDoc '@param' tag has name '{0}', but there is no parameter with that name.",
    },
    DiagnosticMessage {
        code: 8025,
        category: DiagnosticCategory::Error,
        message: "Class declarations cannot have more than one '@augments' or '@extends' tag.",
    },
    DiagnosticMessage {
        code: 8026,
        category: DiagnosticCategory::Error,
        message: "Expected {0} type arguments; provide these with an '@extends' tag.",
    },
    DiagnosticMessage {
        code: 8027,
        category: DiagnosticCategory::Error,
        message: "Expected {0}-{1} type arguments; provide these with an '@extends' tag.",
    },
    DiagnosticMessage {
        code: 8028,
        category: DiagnosticCategory::Error,
        message: "JSDoc '...' may only appear in the last parameter of a signature.",
    },
    DiagnosticMessage {
        code: 8029,
        category: DiagnosticCategory::Error,
        message: "JSDoc '@param' tag has name '{0}', but there is no parameter with that name. It would match 'arguments' if it had an array type.",
    },
    DiagnosticMessage {
        code: 8030,
        category: DiagnosticCategory::Error,
        message: "The type of a function declaration must match the function's signature.",
    },
    DiagnosticMessage {
        code: 8031,
        category: DiagnosticCategory::Error,
        message: "You cannot rename a module via a global import.",
    },
    DiagnosticMessage {
        code: 8032,
        category: DiagnosticCategory::Error,
        message: "Qualified name '{0}' is not allowed without a leading '@param {object} {1}'.",
    },
    DiagnosticMessage {
        code: 8033,
        category: DiagnosticCategory::Error,
        message: "A JSDoc '@typedef' comment may not contain multiple '@type' tags.",
    },
    DiagnosticMessage {
        code: 8034,
        category: DiagnosticCategory::Error,
        message: "The tag was first specified here.",
    },
    DiagnosticMessage {
        code: 8035,
        category: DiagnosticCategory::Error,
        message: "You cannot rename elements that are defined in a 'node_modules' folder.",
    },
    DiagnosticMessage {
        code: 8036,
        category: DiagnosticCategory::Error,
        message: "You cannot rename elements that are defined in another 'node_modules' folder.",
    },
    DiagnosticMessage {
        code: 8037,
        category: DiagnosticCategory::Error,
        message: "Type satisfaction expressions can only be used in TypeScript files.",
    },
    DiagnosticMessage {
        code: 8038,
        category: DiagnosticCategory::Error,
        message: "Decorators may not appear after 'export' or 'export default' if they also appear before 'export'.",
    },
    DiagnosticMessage {
        code: 8039,
        category: DiagnosticCategory::Error,
        message: "A JSDoc '@template' tag may not follow a '@typedef', '@callback', or '@overload' tag",
    },
    DiagnosticMessage {
        code: 9005,
        category: DiagnosticCategory::Error,
        message: "Declaration emit for this file requires using private name '{0}'. An explicit type annotation may unblock declaration emit.",
    },
    DiagnosticMessage {
        code: 9006,
        category: DiagnosticCategory::Error,
        message: "Declaration emit for this file requires using private name '{0}' from module '{1}'. An explicit type annotation may unblock declaration emit.",
    },
    DiagnosticMessage {
        code: 9007,
        category: DiagnosticCategory::Error,
        message: "Function must have an explicit return type annotation with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9008,
        category: DiagnosticCategory::Error,
        message: "Method must have an explicit return type annotation with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9009,
        category: DiagnosticCategory::Error,
        message: "At least one accessor must have an explicit type annotation with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9010,
        category: DiagnosticCategory::Error,
        message: "Variable must have an explicit type annotation with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9011,
        category: DiagnosticCategory::Error,
        message: "Parameter must have an explicit type annotation with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9012,
        category: DiagnosticCategory::Error,
        message: "Property must have an explicit type annotation with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9013,
        category: DiagnosticCategory::Error,
        message: "Expression type can't be inferred with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9014,
        category: DiagnosticCategory::Error,
        message: "Computed properties must be number or string literals, variables or dotted expressions with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9015,
        category: DiagnosticCategory::Error,
        message: "Objects that contain spread assignments can't be inferred with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9016,
        category: DiagnosticCategory::Error,
        message: "Objects that contain shorthand properties can't be inferred with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9017,
        category: DiagnosticCategory::Error,
        message: "Only const arrays can be inferred with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9018,
        category: DiagnosticCategory::Error,
        message: "Arrays with spread elements can't inferred with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9019,
        category: DiagnosticCategory::Error,
        message: "Binding elements can't be exported directly with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9020,
        category: DiagnosticCategory::Error,
        message: "Enum member initializers must be computable without references to external symbols with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9021,
        category: DiagnosticCategory::Error,
        message: "Extends clause can't contain an expression with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9022,
        category: DiagnosticCategory::Error,
        message: "Inference from class expressions is not supported with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9023,
        category: DiagnosticCategory::Error,
        message: "Assigning properties to functions without declaring them is not supported with --isolatedDeclarations. Add an explicit declaration for the properties assigned to this function.",
    },
    DiagnosticMessage {
        code: 9025,
        category: DiagnosticCategory::Error,
        message: "Declaration emit for this parameter requires implicitly adding undefined to its type. This is not supported with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9026,
        category: DiagnosticCategory::Error,
        message: "Declaration emit for this file requires preserving this import for augmentations. This is not supported with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9027,
        category: DiagnosticCategory::Error,
        message: "Add a type annotation to the variable {0}.",
    },
    DiagnosticMessage {
        code: 9028,
        category: DiagnosticCategory::Error,
        message: "Add a type annotation to the parameter {0}.",
    },
    DiagnosticMessage {
        code: 9029,
        category: DiagnosticCategory::Error,
        message: "Add a type annotation to the property {0}.",
    },
    DiagnosticMessage {
        code: 9030,
        category: DiagnosticCategory::Error,
        message: "Add a return type to the function expression.",
    },
    DiagnosticMessage {
        code: 9031,
        category: DiagnosticCategory::Error,
        message: "Add a return type to the function declaration.",
    },
    DiagnosticMessage {
        code: 9032,
        category: DiagnosticCategory::Error,
        message: "Add a return type to the get accessor declaration.",
    },
    DiagnosticMessage {
        code: 9033,
        category: DiagnosticCategory::Error,
        message: "Add a type to parameter of the set accessor declaration.",
    },
    DiagnosticMessage {
        code: 9034,
        category: DiagnosticCategory::Error,
        message: "Add a return type to the method",
    },
    DiagnosticMessage {
        code: 9035,
        category: DiagnosticCategory::Error,
        message: "Add satisfies and a type assertion to this expression (satisfies T as T) to make the type explicit.",
    },
    DiagnosticMessage {
        code: 9036,
        category: DiagnosticCategory::Error,
        message: "Move the expression in default export to a variable and add a type annotation to it.",
    },
    DiagnosticMessage {
        code: 9037,
        category: DiagnosticCategory::Error,
        message: "Default exports can't be inferred with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9038,
        category: DiagnosticCategory::Error,
        message: "Computed property names on class or object literals cannot be inferred with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 9039,
        category: DiagnosticCategory::Error,
        message: "Type containing private name '{0}' can't be used with --isolatedDeclarations.",
    },
    DiagnosticMessage {
        code: 17000,
        category: DiagnosticCategory::Error,
        message: "JSX attributes must only be assigned a non-empty 'expression'.",
    },
    DiagnosticMessage {
        code: 17001,
        category: DiagnosticCategory::Error,
        message: "JSX elements cannot have multiple attributes with the same name.",
    },
    DiagnosticMessage {
        code: 17002,
        category: DiagnosticCategory::Error,
        message: "Expected corresponding JSX closing tag for '{0}'.",
    },
    DiagnosticMessage {
        code: 17004,
        category: DiagnosticCategory::Error,
        message: "Cannot use JSX unless the '--jsx' flag is provided.",
    },
    DiagnosticMessage {
        code: 17005,
        category: DiagnosticCategory::Error,
        message: "A constructor cannot contain a 'super' call when its class extends 'null'.",
    },
    DiagnosticMessage {
        code: 17006,
        category: DiagnosticCategory::Error,
        message: "An unary expression with the '{0}' operator is not allowed in the left-hand side of an exponentiation expression. Consider enclosing the expression in parentheses.",
    },
    DiagnosticMessage {
        code: 17007,
        category: DiagnosticCategory::Error,
        message: "A type assertion expression is not allowed in the left-hand side of an exponentiation expression. Consider enclosing the expression in parentheses.",
    },
    DiagnosticMessage {
        code: 17008,
        category: DiagnosticCategory::Error,
        message: "JSX element '{0}' has no corresponding closing tag.",
    },
    DiagnosticMessage {
        code: 17009,
        category: DiagnosticCategory::Error,
        message: "'super' must be called before accessing 'this' in the constructor of a derived class.",
    },
    DiagnosticMessage {
        code: 17010,
        category: DiagnosticCategory::Error,
        message: "Unknown type acquisition option '{0}'.",
    },
    DiagnosticMessage {
        code: 17011,
        category: DiagnosticCategory::Error,
        message: "'super' must be called before accessing a property of 'super' in the constructor of a derived class.",
    },
    DiagnosticMessage {
        code: 17012,
        category: DiagnosticCategory::Error,
        message: "'{0}' is not a valid meta-property for keyword '{1}'. Did you mean '{2}'?",
    },
    DiagnosticMessage {
        code: 17013,
        category: DiagnosticCategory::Error,
        message: "Meta-property '{0}' is only allowed in the body of a function declaration, function expression, or constructor.",
    },
    DiagnosticMessage {
        code: 17014,
        category: DiagnosticCategory::Error,
        message: "JSX fragment has no corresponding closing tag.",
    },
    DiagnosticMessage {
        code: 17015,
        category: DiagnosticCategory::Error,
        message: "Expected corresponding closing tag for JSX fragment.",
    },
    DiagnosticMessage {
        code: 17016,
        category: DiagnosticCategory::Error,
        message: "The 'jsxFragmentFactory' compiler option must be provided to use JSX fragments with the 'jsxFactory' compiler option.",
    },
    DiagnosticMessage {
        code: 17017,
        category: DiagnosticCategory::Error,
        message: "An @jsxFrag pragma is required when using an @jsx pragma with JSX fragments.",
    },
    DiagnosticMessage {
        code: 17018,
        category: DiagnosticCategory::Error,
        message: "Unknown type acquisition option '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 17019,
        category: DiagnosticCategory::Error,
        message: "'{0}' at the end of a type is not valid TypeScript syntax. Did you mean to write '{1}'?",
    },
    DiagnosticMessage {
        code: 17020,
        category: DiagnosticCategory::Error,
        message: "'{0}' at the start of a type is not valid TypeScript syntax. Did you mean to write '{1}'?",
    },
    DiagnosticMessage {
        code: 17021,
        category: DiagnosticCategory::Error,
        message: "Unicode escape sequence cannot appear here.",
    },
    DiagnosticMessage {
        code: 18000,
        category: DiagnosticCategory::Error,
        message: "Circularity detected while resolving configuration: {0}",
    },
    DiagnosticMessage {
        code: 18002,
        category: DiagnosticCategory::Error,
        message: "The 'files' list in config file '{0}' is empty.",
    },
    DiagnosticMessage {
        code: 18003,
        category: DiagnosticCategory::Error,
        message: "No inputs were found in config file '{0}'. Specified 'include' paths were '{1}' and 'exclude' paths were '{2}'.",
    },
    DiagnosticMessage {
        code: 18004,
        category: DiagnosticCategory::Error,
        message: "No value exists in scope for the shorthand property '{0}'. Either declare one or provide an initializer.",
    },
    DiagnosticMessage {
        code: 18006,
        category: DiagnosticCategory::Error,
        message: "Classes may not have a field named 'constructor'.",
    },
    DiagnosticMessage {
        code: 18007,
        category: DiagnosticCategory::Error,
        message: "JSX expressions may not use the comma operator. Did you mean to write an array?",
    },
    DiagnosticMessage {
        code: 18009,
        category: DiagnosticCategory::Error,
        message: "Private identifiers cannot be used as parameters.",
    },
    DiagnosticMessage {
        code: 18010,
        category: DiagnosticCategory::Error,
        message: "An accessibility modifier cannot be used with a private identifier.",
    },
    DiagnosticMessage {
        code: 18011,
        category: DiagnosticCategory::Error,
        message: "The operand of a 'delete' operator cannot be a private identifier.",
    },
    DiagnosticMessage {
        code: 18012,
        category: DiagnosticCategory::Error,
        message: "'#constructor' is a reserved word.",
    },
    DiagnosticMessage {
        code: 18013,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is not accessible outside class '{1}' because it has a private identifier.",
    },
    DiagnosticMessage {
        code: 18014,
        category: DiagnosticCategory::Error,
        message: "The property '{0}' cannot be accessed on type '{1}' within this class because it is shadowed by another private identifier with the same spelling.",
    },
    DiagnosticMessage {
        code: 18015,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' in type '{1}' refers to a different member that cannot be accessed from within type '{2}'.",
    },
    DiagnosticMessage {
        code: 18016,
        category: DiagnosticCategory::Error,
        message: "Private identifiers are not allowed outside class bodies.",
    },
    DiagnosticMessage {
        code: 18017,
        category: DiagnosticCategory::Error,
        message: "The shadowing declaration of '{0}' is defined here",
    },
    DiagnosticMessage {
        code: 18018,
        category: DiagnosticCategory::Error,
        message: "The declaration of '{0}' that you probably intended to use is defined here",
    },
    DiagnosticMessage {
        code: 18019,
        category: DiagnosticCategory::Error,
        message: "'{0}' modifier cannot be used with a private identifier.",
    },
    DiagnosticMessage {
        code: 18024,
        category: DiagnosticCategory::Error,
        message: "An enum member cannot be named with a private identifier.",
    },
    DiagnosticMessage {
        code: 18026,
        category: DiagnosticCategory::Error,
        message: "'#!' can only be used at the start of a file.",
    },
    DiagnosticMessage {
        code: 18027,
        category: DiagnosticCategory::Error,
        message: "Compiler reserves name '{0}' when emitting private identifier downlevel.",
    },
    DiagnosticMessage {
        code: 18028,
        category: DiagnosticCategory::Error,
        message: "Private identifiers are only available when targeting ECMAScript 2015 and higher.",
    },
    DiagnosticMessage {
        code: 18029,
        category: DiagnosticCategory::Error,
        message: "Private identifiers are not allowed in variable declarations.",
    },
    DiagnosticMessage {
        code: 18030,
        category: DiagnosticCategory::Error,
        message: "An optional chain cannot contain private identifiers.",
    },
    DiagnosticMessage {
        code: 18031,
        category: DiagnosticCategory::Error,
        message: "The intersection '{0}' was reduced to 'never' because property '{1}' has conflicting types in some constituents.",
    },
    DiagnosticMessage {
        code: 18032,
        category: DiagnosticCategory::Error,
        message: "The intersection '{0}' was reduced to 'never' because property '{1}' exists in multiple constituents and is private in some.",
    },
    DiagnosticMessage {
        code: 18033,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}' as required for computed enum member values.",
    },
    DiagnosticMessage {
        code: 18034,
        category: DiagnosticCategory::Message,
        message: "Specify the JSX fragment factory function to use when targeting 'react' JSX emit with 'jsxFactory' compiler option is specified, e.g. 'Fragment'.",
    },
    DiagnosticMessage {
        code: 18035,
        category: DiagnosticCategory::Error,
        message: "Invalid value for 'jsxFragmentFactory'. '{0}' is not a valid identifier or qualified-name.",
    },
    DiagnosticMessage {
        code: 18036,
        category: DiagnosticCategory::Error,
        message: "Class decorators can't be used with static private identifier. Consider removing the experimental decorator.",
    },
    DiagnosticMessage {
        code: 18037,
        category: DiagnosticCategory::Error,
        message: "'await' expression cannot be used inside a class static block.",
    },
    DiagnosticMessage {
        code: 18038,
        category: DiagnosticCategory::Error,
        message: "'for await' loops cannot be used inside a class static block.",
    },
    DiagnosticMessage {
        code: 18039,
        category: DiagnosticCategory::Error,
        message: "Invalid use of '{0}'. It cannot be used inside a class static block.",
    },
    DiagnosticMessage {
        code: 18041,
        category: DiagnosticCategory::Error,
        message: "A 'return' statement cannot be used inside a class static block.",
    },
    DiagnosticMessage {
        code: 18042,
        category: DiagnosticCategory::Error,
        message: "'{0}' is a type and cannot be imported in JavaScript files. Use '{1}' in a JSDoc type annotation.",
    },
    DiagnosticMessage {
        code: 18043,
        category: DiagnosticCategory::Error,
        message: "Types cannot appear in export declarations in JavaScript files.",
    },
    DiagnosticMessage {
        code: 18044,
        category: DiagnosticCategory::Message,
        message: "'{0}' is automatically exported here.",
    },
    DiagnosticMessage {
        code: 18045,
        category: DiagnosticCategory::Error,
        message: "Properties with the 'accessor' modifier are only available when targeting ECMAScript 2015 and higher.",
    },
    DiagnosticMessage {
        code: 18046,
        category: DiagnosticCategory::Error,
        message: "'{0}' is of type 'unknown'.",
    },
    DiagnosticMessage {
        code: 18047,
        category: DiagnosticCategory::Error,
        message: "'{0}' is possibly 'null'.",
    },
    DiagnosticMessage {
        code: 18048,
        category: DiagnosticCategory::Error,
        message: "'{0}' is possibly 'undefined'.",
    },
    DiagnosticMessage {
        code: 18049,
        category: DiagnosticCategory::Error,
        message: "'{0}' is possibly 'null' or 'undefined'.",
    },
    DiagnosticMessage {
        code: 18050,
        category: DiagnosticCategory::Error,
        message: "The value '{0}' cannot be used here.",
    },
    DiagnosticMessage {
        code: 18051,
        category: DiagnosticCategory::Error,
        message: "Compiler option '{0}' cannot be given an empty string.",
    },
    DiagnosticMessage {
        code: 18053,
        category: DiagnosticCategory::Error,
        message: "Its type '{0}' is not a valid JSX element type.",
    },
    DiagnosticMessage {
        code: 18054,
        category: DiagnosticCategory::Error,
        message: "'await using' statements cannot be used inside a class static block.",
    },
    DiagnosticMessage {
        code: 18055,
        category: DiagnosticCategory::Error,
        message: "'{0}' has a string type, but must have syntactically recognizable string syntax when 'isolatedModules' is enabled.",
    },
    DiagnosticMessage {
        code: 18056,
        category: DiagnosticCategory::Error,
        message: "Enum member following a non-literal numeric member must have an initializer when 'isolatedModules' is enabled.",
    },
    DiagnosticMessage {
        code: 18057,
        category: DiagnosticCategory::Error,
        message: "String literal import and export names are not supported when the '--module' flag is set to 'es2015' or 'es2020'.",
    },
    DiagnosticMessage {
        code: 18058,
        category: DiagnosticCategory::Error,
        message: "Default imports are not allowed in a deferred import.",
    },
    DiagnosticMessage {
        code: 18059,
        category: DiagnosticCategory::Error,
        message: "Named imports are not allowed in a deferred import.",
    },
    DiagnosticMessage {
        code: 18060,
        category: DiagnosticCategory::Error,
        message: "Deferred imports are only supported when the '--module' flag is set to 'esnext' or 'preserve'.",
    },
    DiagnosticMessage {
        code: 18061,
        category: DiagnosticCategory::Error,
        message: "'{0}' is not a valid meta-property for keyword 'import'. Did you mean 'meta' or 'defer'?",
    },
    DiagnosticMessage {
        code: 69010,
        category: DiagnosticCategory::Message,
        message: "`nodenext` if `module` is `nodenext`; `node16` if `module` is `node16` or `node18`; otherwise, `bundler`.",
    },
    DiagnosticMessage {
        code: 80001,
        category: DiagnosticCategory::Suggestion,
        message: "File is a CommonJS module; it may be converted to an ES module.",
    },
    DiagnosticMessage {
        code: 80002,
        category: DiagnosticCategory::Suggestion,
        message: "This constructor function may be converted to a class declaration.",
    },
    DiagnosticMessage {
        code: 80003,
        category: DiagnosticCategory::Suggestion,
        message: "Import may be converted to a default import.",
    },
    DiagnosticMessage {
        code: 80004,
        category: DiagnosticCategory::Suggestion,
        message: "JSDoc types may be moved to TypeScript types.",
    },
    DiagnosticMessage {
        code: 80005,
        category: DiagnosticCategory::Suggestion,
        message: "'require' call may be converted to an import.",
    },
    DiagnosticMessage {
        code: 80006,
        category: DiagnosticCategory::Suggestion,
        message: "This may be converted to an async function.",
    },
    DiagnosticMessage {
        code: 80007,
        category: DiagnosticCategory::Suggestion,
        message: "'await' has no effect on the type of this expression.",
    },
    DiagnosticMessage {
        code: 80008,
        category: DiagnosticCategory::Suggestion,
        message: "Numeric literals with absolute values equal to 2^53 or greater are too large to be represented accurately as integers.",
    },
    DiagnosticMessage {
        code: 80009,
        category: DiagnosticCategory::Suggestion,
        message: "JSDoc typedef may be converted to TypeScript type.",
    },
    DiagnosticMessage {
        code: 80010,
        category: DiagnosticCategory::Suggestion,
        message: "JSDoc typedefs may be converted to TypeScript types.",
    },
    DiagnosticMessage {
        code: 90001,
        category: DiagnosticCategory::Message,
        message: "Add missing 'super()' call",
    },
    DiagnosticMessage {
        code: 90002,
        category: DiagnosticCategory::Message,
        message: "Make 'super()' call the first statement in the constructor",
    },
    DiagnosticMessage {
        code: 90003,
        category: DiagnosticCategory::Message,
        message: "Change 'extends' to 'implements'",
    },
    DiagnosticMessage {
        code: 90004,
        category: DiagnosticCategory::Message,
        message: "Remove unused declaration for: '{0}'",
    },
    DiagnosticMessage {
        code: 90005,
        category: DiagnosticCategory::Message,
        message: "Remove import from '{0}'",
    },
    DiagnosticMessage {
        code: 90006,
        category: DiagnosticCategory::Message,
        message: "Implement interface '{0}'",
    },
    DiagnosticMessage {
        code: 90007,
        category: DiagnosticCategory::Message,
        message: "Implement inherited abstract class",
    },
    DiagnosticMessage {
        code: 90008,
        category: DiagnosticCategory::Message,
        message: "Add '{0}.' to unresolved variable",
    },
    DiagnosticMessage {
        code: 90010,
        category: DiagnosticCategory::Message,
        message: "Remove variable statement",
    },
    DiagnosticMessage {
        code: 90011,
        category: DiagnosticCategory::Message,
        message: "Remove template tag",
    },
    DiagnosticMessage {
        code: 90012,
        category: DiagnosticCategory::Message,
        message: "Remove type parameters",
    },
    DiagnosticMessage {
        code: 90013,
        category: DiagnosticCategory::Message,
        message: "Import '{0}' from \"{1}\"",
    },
    DiagnosticMessage {
        code: 90014,
        category: DiagnosticCategory::Message,
        message: "Change '{0}' to '{1}'",
    },
    DiagnosticMessage {
        code: 90016,
        category: DiagnosticCategory::Message,
        message: "Declare property '{0}'",
    },
    DiagnosticMessage {
        code: 90017,
        category: DiagnosticCategory::Message,
        message: "Add index signature for property '{0}'",
    },
    DiagnosticMessage {
        code: 90018,
        category: DiagnosticCategory::Message,
        message: "Disable checking for this file",
    },
    DiagnosticMessage {
        code: 90019,
        category: DiagnosticCategory::Message,
        message: "Ignore this error message",
    },
    DiagnosticMessage {
        code: 90020,
        category: DiagnosticCategory::Message,
        message: "Initialize property '{0}' in the constructor",
    },
    DiagnosticMessage {
        code: 90021,
        category: DiagnosticCategory::Message,
        message: "Initialize static property '{0}'",
    },
    DiagnosticMessage {
        code: 90022,
        category: DiagnosticCategory::Message,
        message: "Change spelling to '{0}'",
    },
    DiagnosticMessage {
        code: 90023,
        category: DiagnosticCategory::Message,
        message: "Declare method '{0}'",
    },
    DiagnosticMessage {
        code: 90024,
        category: DiagnosticCategory::Message,
        message: "Declare static method '{0}'",
    },
    DiagnosticMessage {
        code: 90025,
        category: DiagnosticCategory::Message,
        message: "Prefix '{0}' with an underscore",
    },
    DiagnosticMessage {
        code: 90026,
        category: DiagnosticCategory::Message,
        message: "Rewrite as the indexed access type '{0}'",
    },
    DiagnosticMessage {
        code: 90027,
        category: DiagnosticCategory::Message,
        message: "Declare static property '{0}'",
    },
    DiagnosticMessage {
        code: 90028,
        category: DiagnosticCategory::Message,
        message: "Call decorator expression",
    },
    DiagnosticMessage {
        code: 90029,
        category: DiagnosticCategory::Message,
        message: "Add async modifier to containing function",
    },
    DiagnosticMessage {
        code: 90030,
        category: DiagnosticCategory::Message,
        message: "Replace 'infer {0}' with 'unknown'",
    },
    DiagnosticMessage {
        code: 90031,
        category: DiagnosticCategory::Message,
        message: "Replace all unused 'infer' with 'unknown'",
    },
    DiagnosticMessage {
        code: 90034,
        category: DiagnosticCategory::Message,
        message: "Add parameter name",
    },
    DiagnosticMessage {
        code: 90035,
        category: DiagnosticCategory::Message,
        message: "Declare private property '{0}'",
    },
    DiagnosticMessage {
        code: 90036,
        category: DiagnosticCategory::Message,
        message: "Replace '{0}' with 'Promise<{1}>'",
    },
    DiagnosticMessage {
        code: 90037,
        category: DiagnosticCategory::Message,
        message: "Fix all incorrect return type of an async functions",
    },
    DiagnosticMessage {
        code: 90038,
        category: DiagnosticCategory::Message,
        message: "Declare private method '{0}'",
    },
    DiagnosticMessage {
        code: 90039,
        category: DiagnosticCategory::Message,
        message: "Remove unused destructuring declaration",
    },
    DiagnosticMessage {
        code: 90041,
        category: DiagnosticCategory::Message,
        message: "Remove unused declarations for: '{0}'",
    },
    DiagnosticMessage {
        code: 90053,
        category: DiagnosticCategory::Message,
        message: "Declare a private field named '{0}'.",
    },
    DiagnosticMessage {
        code: 90054,
        category: DiagnosticCategory::Message,
        message: "Includes imports of types referenced by '{0}'",
    },
    DiagnosticMessage {
        code: 90055,
        category: DiagnosticCategory::Message,
        message: "Remove 'type' from import declaration from \"{0}\"",
    },
    DiagnosticMessage {
        code: 90056,
        category: DiagnosticCategory::Message,
        message: "Remove 'type' from import of '{0}' from \"{1}\"",
    },
    DiagnosticMessage {
        code: 90057,
        category: DiagnosticCategory::Message,
        message: "Add import from \"{0}\"",
    },
    DiagnosticMessage {
        code: 90058,
        category: DiagnosticCategory::Message,
        message: "Update import from \"{0}\"",
    },
    DiagnosticMessage {
        code: 90059,
        category: DiagnosticCategory::Message,
        message: "Export '{0}' from module '{1}'",
    },
    DiagnosticMessage {
        code: 90060,
        category: DiagnosticCategory::Message,
        message: "Export all referenced locals",
    },
    DiagnosticMessage {
        code: 90061,
        category: DiagnosticCategory::Message,
        message: "Update modifiers of '{0}'",
    },
    DiagnosticMessage {
        code: 90062,
        category: DiagnosticCategory::Message,
        message: "Add annotation of type '{0}'",
    },
    DiagnosticMessage {
        code: 90063,
        category: DiagnosticCategory::Message,
        message: "Add return type '{0}'",
    },
    DiagnosticMessage {
        code: 90064,
        category: DiagnosticCategory::Message,
        message: "Extract base class to variable",
    },
    DiagnosticMessage {
        code: 90065,
        category: DiagnosticCategory::Message,
        message: "Extract default export to variable",
    },
    DiagnosticMessage {
        code: 90066,
        category: DiagnosticCategory::Message,
        message: "Extract binding expressions to variable",
    },
    DiagnosticMessage {
        code: 90067,
        category: DiagnosticCategory::Message,
        message: "Add all missing type annotations",
    },
    DiagnosticMessage {
        code: 90068,
        category: DiagnosticCategory::Message,
        message: "Add satisfies and an inline type assertion with '{0}'",
    },
    DiagnosticMessage {
        code: 90069,
        category: DiagnosticCategory::Message,
        message: "Extract to variable and replace with '{0} as typeof {0}'",
    },
    DiagnosticMessage {
        code: 90070,
        category: DiagnosticCategory::Message,
        message: "Mark array literal as const",
    },
    DiagnosticMessage {
        code: 90071,
        category: DiagnosticCategory::Message,
        message: "Annotate types of properties expando function in a namespace",
    },
    DiagnosticMessage {
        code: 95001,
        category: DiagnosticCategory::Message,
        message: "Convert function to an ES2015 class",
    },
    DiagnosticMessage {
        code: 95003,
        category: DiagnosticCategory::Message,
        message: "Convert '{0}' to '{1} in {0}'",
    },
    DiagnosticMessage {
        code: 95004,
        category: DiagnosticCategory::Message,
        message: "Extract to {0} in {1}",
    },
    DiagnosticMessage {
        code: 95005,
        category: DiagnosticCategory::Message,
        message: "Extract function",
    },
    DiagnosticMessage {
        code: 95006,
        category: DiagnosticCategory::Message,
        message: "Extract constant",
    },
    DiagnosticMessage {
        code: 95007,
        category: DiagnosticCategory::Message,
        message: "Extract to {0} in enclosing scope",
    },
    DiagnosticMessage {
        code: 95008,
        category: DiagnosticCategory::Message,
        message: "Extract to {0} in {1} scope",
    },
    DiagnosticMessage {
        code: 95009,
        category: DiagnosticCategory::Message,
        message: "Annotate with type from JSDoc",
    },
    DiagnosticMessage {
        code: 95011,
        category: DiagnosticCategory::Message,
        message: "Infer type of '{0}' from usage",
    },
    DiagnosticMessage {
        code: 95012,
        category: DiagnosticCategory::Message,
        message: "Infer parameter types from usage",
    },
    DiagnosticMessage {
        code: 95013,
        category: DiagnosticCategory::Message,
        message: "Convert to default import",
    },
    DiagnosticMessage {
        code: 95014,
        category: DiagnosticCategory::Message,
        message: "Install '{0}'",
    },
    DiagnosticMessage {
        code: 95015,
        category: DiagnosticCategory::Message,
        message: "Replace import with '{0}'.",
    },
    DiagnosticMessage {
        code: 95016,
        category: DiagnosticCategory::Message,
        message: "Use synthetic 'default' member.",
    },
    DiagnosticMessage {
        code: 95017,
        category: DiagnosticCategory::Message,
        message: "Convert to ES module",
    },
    DiagnosticMessage {
        code: 95018,
        category: DiagnosticCategory::Message,
        message: "Add 'undefined' type to property '{0}'",
    },
    DiagnosticMessage {
        code: 95019,
        category: DiagnosticCategory::Message,
        message: "Add initializer to property '{0}'",
    },
    DiagnosticMessage {
        code: 95020,
        category: DiagnosticCategory::Message,
        message: "Add definite assignment assertion to property '{0}'",
    },
    DiagnosticMessage {
        code: 95021,
        category: DiagnosticCategory::Message,
        message: "Convert all type literals to mapped type",
    },
    DiagnosticMessage {
        code: 95022,
        category: DiagnosticCategory::Message,
        message: "Add all missing members",
    },
    DiagnosticMessage {
        code: 95023,
        category: DiagnosticCategory::Message,
        message: "Infer all types from usage",
    },
    DiagnosticMessage {
        code: 95024,
        category: DiagnosticCategory::Message,
        message: "Delete all unused declarations",
    },
    DiagnosticMessage {
        code: 95025,
        category: DiagnosticCategory::Message,
        message: "Prefix all unused declarations with '_' where possible",
    },
    DiagnosticMessage {
        code: 95026,
        category: DiagnosticCategory::Message,
        message: "Fix all detected spelling errors",
    },
    DiagnosticMessage {
        code: 95027,
        category: DiagnosticCategory::Message,
        message: "Add initializers to all uninitialized properties",
    },
    DiagnosticMessage {
        code: 95028,
        category: DiagnosticCategory::Message,
        message: "Add definite assignment assertions to all uninitialized properties",
    },
    DiagnosticMessage {
        code: 95029,
        category: DiagnosticCategory::Message,
        message: "Add undefined type to all uninitialized properties",
    },
    DiagnosticMessage {
        code: 95030,
        category: DiagnosticCategory::Message,
        message: "Change all jsdoc-style types to TypeScript",
    },
    DiagnosticMessage {
        code: 95031,
        category: DiagnosticCategory::Message,
        message: "Change all jsdoc-style types to TypeScript (and add '| undefined' to nullable types)",
    },
    DiagnosticMessage {
        code: 95032,
        category: DiagnosticCategory::Message,
        message: "Implement all unimplemented interfaces",
    },
    DiagnosticMessage {
        code: 95033,
        category: DiagnosticCategory::Message,
        message: "Install all missing types packages",
    },
    DiagnosticMessage {
        code: 95034,
        category: DiagnosticCategory::Message,
        message: "Rewrite all as indexed access types",
    },
    DiagnosticMessage {
        code: 95035,
        category: DiagnosticCategory::Message,
        message: "Convert all to default imports",
    },
    DiagnosticMessage {
        code: 95036,
        category: DiagnosticCategory::Message,
        message: "Make all 'super()' calls the first statement in their constructor",
    },
    DiagnosticMessage {
        code: 95037,
        category: DiagnosticCategory::Message,
        message: "Add qualifier to all unresolved variables matching a member name",
    },
    DiagnosticMessage {
        code: 95038,
        category: DiagnosticCategory::Message,
        message: "Change all extended interfaces to 'implements'",
    },
    DiagnosticMessage {
        code: 95039,
        category: DiagnosticCategory::Message,
        message: "Add all missing super calls",
    },
    DiagnosticMessage {
        code: 95040,
        category: DiagnosticCategory::Message,
        message: "Implement all inherited abstract classes",
    },
    DiagnosticMessage {
        code: 95041,
        category: DiagnosticCategory::Message,
        message: "Add all missing 'async' modifiers",
    },
    DiagnosticMessage {
        code: 95042,
        category: DiagnosticCategory::Message,
        message: "Add '@ts-ignore' to all error messages",
    },
    DiagnosticMessage {
        code: 95043,
        category: DiagnosticCategory::Message,
        message: "Annotate everything with types from JSDoc",
    },
    DiagnosticMessage {
        code: 95044,
        category: DiagnosticCategory::Message,
        message: "Add '()' to all uncalled decorators",
    },
    DiagnosticMessage {
        code: 95045,
        category: DiagnosticCategory::Message,
        message: "Convert all constructor functions to classes",
    },
    DiagnosticMessage {
        code: 95046,
        category: DiagnosticCategory::Message,
        message: "Generate 'get' and 'set' accessors",
    },
    DiagnosticMessage {
        code: 95047,
        category: DiagnosticCategory::Message,
        message: "Convert 'require' to 'import'",
    },
    DiagnosticMessage {
        code: 95048,
        category: DiagnosticCategory::Message,
        message: "Convert all 'require' to 'import'",
    },
    DiagnosticMessage {
        code: 95049,
        category: DiagnosticCategory::Message,
        message: "Move to a new file",
    },
    DiagnosticMessage {
        code: 95050,
        category: DiagnosticCategory::Message,
        message: "Remove unreachable code",
    },
    DiagnosticMessage {
        code: 95051,
        category: DiagnosticCategory::Message,
        message: "Remove all unreachable code",
    },
    DiagnosticMessage {
        code: 95052,
        category: DiagnosticCategory::Message,
        message: "Add missing 'typeof'",
    },
    DiagnosticMessage {
        code: 95053,
        category: DiagnosticCategory::Message,
        message: "Remove unused label",
    },
    DiagnosticMessage {
        code: 95054,
        category: DiagnosticCategory::Message,
        message: "Remove all unused labels",
    },
    DiagnosticMessage {
        code: 95055,
        category: DiagnosticCategory::Message,
        message: "Convert '{0}' to mapped object type",
    },
    DiagnosticMessage {
        code: 95056,
        category: DiagnosticCategory::Message,
        message: "Convert namespace import to named imports",
    },
    DiagnosticMessage {
        code: 95057,
        category: DiagnosticCategory::Message,
        message: "Convert named imports to namespace import",
    },
    DiagnosticMessage {
        code: 95058,
        category: DiagnosticCategory::Message,
        message: "Add or remove braces in an arrow function",
    },
    DiagnosticMessage {
        code: 95059,
        category: DiagnosticCategory::Message,
        message: "Add braces to arrow function",
    },
    DiagnosticMessage {
        code: 95060,
        category: DiagnosticCategory::Message,
        message: "Remove braces from arrow function",
    },
    DiagnosticMessage {
        code: 95061,
        category: DiagnosticCategory::Message,
        message: "Convert default export to named export",
    },
    DiagnosticMessage {
        code: 95062,
        category: DiagnosticCategory::Message,
        message: "Convert named export to default export",
    },
    DiagnosticMessage {
        code: 95063,
        category: DiagnosticCategory::Message,
        message: "Add missing enum member '{0}'",
    },
    DiagnosticMessage {
        code: 95064,
        category: DiagnosticCategory::Message,
        message: "Add all missing imports",
    },
    DiagnosticMessage {
        code: 95065,
        category: DiagnosticCategory::Message,
        message: "Convert to async function",
    },
    DiagnosticMessage {
        code: 95066,
        category: DiagnosticCategory::Message,
        message: "Convert all to async functions",
    },
    DiagnosticMessage {
        code: 95067,
        category: DiagnosticCategory::Message,
        message: "Add missing call parentheses",
    },
    DiagnosticMessage {
        code: 95068,
        category: DiagnosticCategory::Message,
        message: "Add all missing call parentheses",
    },
    DiagnosticMessage {
        code: 95069,
        category: DiagnosticCategory::Message,
        message: "Add 'unknown' conversion for non-overlapping types",
    },
    DiagnosticMessage {
        code: 95070,
        category: DiagnosticCategory::Message,
        message: "Add 'unknown' to all conversions of non-overlapping types",
    },
    DiagnosticMessage {
        code: 95071,
        category: DiagnosticCategory::Message,
        message: "Add missing 'new' operator to call",
    },
    DiagnosticMessage {
        code: 95072,
        category: DiagnosticCategory::Message,
        message: "Add missing 'new' operator to all calls",
    },
    DiagnosticMessage {
        code: 95073,
        category: DiagnosticCategory::Message,
        message: "Add names to all parameters without names",
    },
    DiagnosticMessage {
        code: 95074,
        category: DiagnosticCategory::Message,
        message: "Enable the 'experimentalDecorators' option in your configuration file",
    },
    DiagnosticMessage {
        code: 95075,
        category: DiagnosticCategory::Message,
        message: "Convert parameters to destructured object",
    },
    DiagnosticMessage {
        code: 95077,
        category: DiagnosticCategory::Message,
        message: "Extract type",
    },
    DiagnosticMessage {
        code: 95078,
        category: DiagnosticCategory::Message,
        message: "Extract to type alias",
    },
    DiagnosticMessage {
        code: 95079,
        category: DiagnosticCategory::Message,
        message: "Extract to typedef",
    },
    DiagnosticMessage {
        code: 95080,
        category: DiagnosticCategory::Message,
        message: "Infer 'this' type of '{0}' from usage",
    },
    DiagnosticMessage {
        code: 95081,
        category: DiagnosticCategory::Message,
        message: "Add 'const' to unresolved variable",
    },
    DiagnosticMessage {
        code: 95082,
        category: DiagnosticCategory::Message,
        message: "Add 'const' to all unresolved variables",
    },
    DiagnosticMessage {
        code: 95083,
        category: DiagnosticCategory::Message,
        message: "Add 'await'",
    },
    DiagnosticMessage {
        code: 95084,
        category: DiagnosticCategory::Message,
        message: "Add 'await' to initializer for '{0}'",
    },
    DiagnosticMessage {
        code: 95085,
        category: DiagnosticCategory::Message,
        message: "Fix all expressions possibly missing 'await'",
    },
    DiagnosticMessage {
        code: 95086,
        category: DiagnosticCategory::Message,
        message: "Remove unnecessary 'await'",
    },
    DiagnosticMessage {
        code: 95087,
        category: DiagnosticCategory::Message,
        message: "Remove all unnecessary uses of 'await'",
    },
    DiagnosticMessage {
        code: 95088,
        category: DiagnosticCategory::Message,
        message: "Enable the '--jsx' flag in your configuration file",
    },
    DiagnosticMessage {
        code: 95089,
        category: DiagnosticCategory::Message,
        message: "Add 'await' to initializers",
    },
    DiagnosticMessage {
        code: 95090,
        category: DiagnosticCategory::Message,
        message: "Extract to interface",
    },
    DiagnosticMessage {
        code: 95091,
        category: DiagnosticCategory::Message,
        message: "Convert to a bigint numeric literal",
    },
    DiagnosticMessage {
        code: 95092,
        category: DiagnosticCategory::Message,
        message: "Convert all to bigint numeric literals",
    },
    DiagnosticMessage {
        code: 95093,
        category: DiagnosticCategory::Message,
        message: "Convert 'const' to 'let'",
    },
    DiagnosticMessage {
        code: 95094,
        category: DiagnosticCategory::Message,
        message: "Prefix with 'declare'",
    },
    DiagnosticMessage {
        code: 95095,
        category: DiagnosticCategory::Message,
        message: "Prefix all incorrect property declarations with 'declare'",
    },
    DiagnosticMessage {
        code: 95096,
        category: DiagnosticCategory::Message,
        message: "Convert to template string",
    },
    DiagnosticMessage {
        code: 95097,
        category: DiagnosticCategory::Message,
        message: "Add 'export {}' to make this file into a module",
    },
    DiagnosticMessage {
        code: 95098,
        category: DiagnosticCategory::Message,
        message: "Set the 'target' option in your configuration file to '{0}'",
    },
    DiagnosticMessage {
        code: 95099,
        category: DiagnosticCategory::Message,
        message: "Set the 'module' option in your configuration file to '{0}'",
    },
    DiagnosticMessage {
        code: 95100,
        category: DiagnosticCategory::Message,
        message: "Convert invalid character to its html entity code",
    },
    DiagnosticMessage {
        code: 95101,
        category: DiagnosticCategory::Message,
        message: "Convert all invalid characters to HTML entity code",
    },
    DiagnosticMessage {
        code: 95102,
        category: DiagnosticCategory::Message,
        message: "Convert all 'const' to 'let'",
    },
    DiagnosticMessage {
        code: 95105,
        category: DiagnosticCategory::Message,
        message: "Convert function expression '{0}' to arrow function",
    },
    DiagnosticMessage {
        code: 95106,
        category: DiagnosticCategory::Message,
        message: "Convert function declaration '{0}' to arrow function",
    },
    DiagnosticMessage {
        code: 95107,
        category: DiagnosticCategory::Message,
        message: "Fix all implicit-'this' errors",
    },
    DiagnosticMessage {
        code: 95108,
        category: DiagnosticCategory::Message,
        message: "Wrap invalid character in an expression container",
    },
    DiagnosticMessage {
        code: 95109,
        category: DiagnosticCategory::Message,
        message: "Wrap all invalid characters in an expression container",
    },
    DiagnosticMessage {
        code: 95110,
        category: DiagnosticCategory::Message,
        message: "Visit https://aka.ms/tsconfig to read more about this file",
    },
    DiagnosticMessage {
        code: 95111,
        category: DiagnosticCategory::Message,
        message: "Add a return statement",
    },
    DiagnosticMessage {
        code: 95112,
        category: DiagnosticCategory::Message,
        message: "Remove braces from arrow function body",
    },
    DiagnosticMessage {
        code: 95113,
        category: DiagnosticCategory::Message,
        message: "Wrap the following body with parentheses which should be an object literal",
    },
    DiagnosticMessage {
        code: 95114,
        category: DiagnosticCategory::Message,
        message: "Add all missing return statement",
    },
    DiagnosticMessage {
        code: 95115,
        category: DiagnosticCategory::Message,
        message: "Remove braces from all arrow function bodies with relevant issues",
    },
    DiagnosticMessage {
        code: 95116,
        category: DiagnosticCategory::Message,
        message: "Wrap all object literal with parentheses",
    },
    DiagnosticMessage {
        code: 95117,
        category: DiagnosticCategory::Message,
        message: "Move labeled tuple element modifiers to labels",
    },
    DiagnosticMessage {
        code: 95118,
        category: DiagnosticCategory::Message,
        message: "Convert overload list to single signature",
    },
    DiagnosticMessage {
        code: 95119,
        category: DiagnosticCategory::Message,
        message: "Generate 'get' and 'set' accessors for all overriding properties",
    },
    DiagnosticMessage {
        code: 95120,
        category: DiagnosticCategory::Message,
        message: "Wrap in JSX fragment",
    },
    DiagnosticMessage {
        code: 95121,
        category: DiagnosticCategory::Message,
        message: "Wrap all unparented JSX in JSX fragment",
    },
    DiagnosticMessage {
        code: 95122,
        category: DiagnosticCategory::Message,
        message: "Convert arrow function or function expression",
    },
    DiagnosticMessage {
        code: 95123,
        category: DiagnosticCategory::Message,
        message: "Convert to anonymous function",
    },
    DiagnosticMessage {
        code: 95124,
        category: DiagnosticCategory::Message,
        message: "Convert to named function",
    },
    DiagnosticMessage {
        code: 95125,
        category: DiagnosticCategory::Message,
        message: "Convert to arrow function",
    },
    DiagnosticMessage {
        code: 95126,
        category: DiagnosticCategory::Message,
        message: "Remove parentheses",
    },
    DiagnosticMessage {
        code: 95127,
        category: DiagnosticCategory::Message,
        message: "Could not find a containing arrow function",
    },
    DiagnosticMessage {
        code: 95128,
        category: DiagnosticCategory::Message,
        message: "Containing function is not an arrow function",
    },
    DiagnosticMessage {
        code: 95129,
        category: DiagnosticCategory::Message,
        message: "Could not find export statement",
    },
    DiagnosticMessage {
        code: 95130,
        category: DiagnosticCategory::Message,
        message: "This file already has a default export",
    },
    DiagnosticMessage {
        code: 95131,
        category: DiagnosticCategory::Message,
        message: "Could not find import clause",
    },
    DiagnosticMessage {
        code: 95132,
        category: DiagnosticCategory::Message,
        message: "Could not find namespace import or named imports",
    },
    DiagnosticMessage {
        code: 95133,
        category: DiagnosticCategory::Message,
        message: "Selection is not a valid type node",
    },
    DiagnosticMessage {
        code: 95134,
        category: DiagnosticCategory::Message,
        message: "No type could be extracted from this type node",
    },
    DiagnosticMessage {
        code: 95135,
        category: DiagnosticCategory::Message,
        message: "Could not find property for which to generate accessor",
    },
    DiagnosticMessage {
        code: 95136,
        category: DiagnosticCategory::Message,
        message: "Name is not valid",
    },
    DiagnosticMessage {
        code: 95137,
        category: DiagnosticCategory::Message,
        message: "Can only convert property with modifier",
    },
    DiagnosticMessage {
        code: 95138,
        category: DiagnosticCategory::Message,
        message: "Switch each misused '{0}' to '{1}'",
    },
    DiagnosticMessage {
        code: 95139,
        category: DiagnosticCategory::Message,
        message: "Convert to optional chain expression",
    },
    DiagnosticMessage {
        code: 95140,
        category: DiagnosticCategory::Message,
        message: "Could not find convertible access expression",
    },
    DiagnosticMessage {
        code: 95141,
        category: DiagnosticCategory::Message,
        message: "Could not find matching access expressions",
    },
    DiagnosticMessage {
        code: 95142,
        category: DiagnosticCategory::Message,
        message: "Can only convert logical AND access chains",
    },
    DiagnosticMessage {
        code: 95143,
        category: DiagnosticCategory::Message,
        message: "Add 'void' to Promise resolved without a value",
    },
    DiagnosticMessage {
        code: 95144,
        category: DiagnosticCategory::Message,
        message: "Add 'void' to all Promises resolved without a value",
    },
    DiagnosticMessage {
        code: 95145,
        category: DiagnosticCategory::Message,
        message: "Use element access for '{0}'",
    },
    DiagnosticMessage {
        code: 95146,
        category: DiagnosticCategory::Message,
        message: "Use element access for all undeclared properties.",
    },
    DiagnosticMessage {
        code: 95147,
        category: DiagnosticCategory::Message,
        message: "Delete all unused imports",
    },
    DiagnosticMessage {
        code: 95148,
        category: DiagnosticCategory::Message,
        message: "Infer function return type",
    },
    DiagnosticMessage {
        code: 95149,
        category: DiagnosticCategory::Message,
        message: "Return type must be inferred from a function",
    },
    DiagnosticMessage {
        code: 95150,
        category: DiagnosticCategory::Message,
        message: "Could not determine function return type",
    },
    DiagnosticMessage {
        code: 95151,
        category: DiagnosticCategory::Message,
        message: "Could not convert to arrow function",
    },
    DiagnosticMessage {
        code: 95152,
        category: DiagnosticCategory::Message,
        message: "Could not convert to named function",
    },
    DiagnosticMessage {
        code: 95153,
        category: DiagnosticCategory::Message,
        message: "Could not convert to anonymous function",
    },
    DiagnosticMessage {
        code: 95154,
        category: DiagnosticCategory::Message,
        message: "Can only convert string concatenations and string literals",
    },
    DiagnosticMessage {
        code: 95155,
        category: DiagnosticCategory::Message,
        message: "Selection is not a valid statement or statements",
    },
    DiagnosticMessage {
        code: 95156,
        category: DiagnosticCategory::Message,
        message: "Add missing function declaration '{0}'",
    },
    DiagnosticMessage {
        code: 95157,
        category: DiagnosticCategory::Message,
        message: "Add all missing function declarations",
    },
    DiagnosticMessage {
        code: 95158,
        category: DiagnosticCategory::Message,
        message: "Method not implemented.",
    },
    DiagnosticMessage {
        code: 95159,
        category: DiagnosticCategory::Message,
        message: "Function not implemented.",
    },
    DiagnosticMessage {
        code: 95160,
        category: DiagnosticCategory::Message,
        message: "Add 'override' modifier",
    },
    DiagnosticMessage {
        code: 95161,
        category: DiagnosticCategory::Message,
        message: "Remove 'override' modifier",
    },
    DiagnosticMessage {
        code: 95162,
        category: DiagnosticCategory::Message,
        message: "Add all missing 'override' modifiers",
    },
    DiagnosticMessage {
        code: 95163,
        category: DiagnosticCategory::Message,
        message: "Remove all unnecessary 'override' modifiers",
    },
    DiagnosticMessage {
        code: 95164,
        category: DiagnosticCategory::Message,
        message: "Can only convert named export",
    },
    DiagnosticMessage {
        code: 95165,
        category: DiagnosticCategory::Message,
        message: "Add missing properties",
    },
    DiagnosticMessage {
        code: 95166,
        category: DiagnosticCategory::Message,
        message: "Add all missing properties",
    },
    DiagnosticMessage {
        code: 95167,
        category: DiagnosticCategory::Message,
        message: "Add missing attributes",
    },
    DiagnosticMessage {
        code: 95168,
        category: DiagnosticCategory::Message,
        message: "Add all missing attributes",
    },
    DiagnosticMessage {
        code: 95169,
        category: DiagnosticCategory::Message,
        message: "Add 'undefined' to optional property type",
    },
    DiagnosticMessage {
        code: 95170,
        category: DiagnosticCategory::Message,
        message: "Convert named imports to default import",
    },
    DiagnosticMessage {
        code: 95171,
        category: DiagnosticCategory::Message,
        message: "Delete unused '@param' tag '{0}'",
    },
    DiagnosticMessage {
        code: 95172,
        category: DiagnosticCategory::Message,
        message: "Delete all unused '@param' tags",
    },
    DiagnosticMessage {
        code: 95173,
        category: DiagnosticCategory::Message,
        message: "Rename '@param' tag name '{0}' to '{1}'",
    },
    DiagnosticMessage {
        code: 95174,
        category: DiagnosticCategory::Message,
        message: "Use `{0}`.",
    },
    DiagnosticMessage {
        code: 95175,
        category: DiagnosticCategory::Message,
        message: "Use `Number.isNaN` in all conditions.",
    },
    DiagnosticMessage {
        code: 95176,
        category: DiagnosticCategory::Message,
        message: "Convert typedef to TypeScript type.",
    },
    DiagnosticMessage {
        code: 95177,
        category: DiagnosticCategory::Message,
        message: "Convert all typedef to TypeScript types.",
    },
    DiagnosticMessage {
        code: 95178,
        category: DiagnosticCategory::Message,
        message: "Move to file",
    },
    DiagnosticMessage {
        code: 95179,
        category: DiagnosticCategory::Message,
        message: "Cannot move to file, selected file is invalid",
    },
    DiagnosticMessage {
        code: 95180,
        category: DiagnosticCategory::Message,
        message: "Use 'import type'",
    },
    DiagnosticMessage {
        code: 95181,
        category: DiagnosticCategory::Message,
        message: "Use 'type {0}'",
    },
    DiagnosticMessage {
        code: 95182,
        category: DiagnosticCategory::Message,
        message: "Fix all with type-only imports",
    },
    DiagnosticMessage {
        code: 95183,
        category: DiagnosticCategory::Message,
        message: "Cannot move statements to the selected file",
    },
    DiagnosticMessage {
        code: 95184,
        category: DiagnosticCategory::Message,
        message: "Inline variable",
    },
    DiagnosticMessage {
        code: 95185,
        category: DiagnosticCategory::Message,
        message: "Could not find variable to inline.",
    },
    DiagnosticMessage {
        code: 95186,
        category: DiagnosticCategory::Message,
        message: "Variables with multiple declarations cannot be inlined.",
    },
    DiagnosticMessage {
        code: 95187,
        category: DiagnosticCategory::Message,
        message: "Add missing comma for object member completion '{0}'.",
    },
    DiagnosticMessage {
        code: 95188,
        category: DiagnosticCategory::Message,
        message: "Add missing parameter to '{0}'",
    },
    DiagnosticMessage {
        code: 95189,
        category: DiagnosticCategory::Message,
        message: "Add missing parameters to '{0}'",
    },
    DiagnosticMessage {
        code: 95190,
        category: DiagnosticCategory::Message,
        message: "Add all missing parameters",
    },
    DiagnosticMessage {
        code: 95191,
        category: DiagnosticCategory::Message,
        message: "Add optional parameter to '{0}'",
    },
    DiagnosticMessage {
        code: 95192,
        category: DiagnosticCategory::Message,
        message: "Add optional parameters to '{0}'",
    },
    DiagnosticMessage {
        code: 95193,
        category: DiagnosticCategory::Message,
        message: "Add all optional parameters",
    },
    DiagnosticMessage {
        code: 95194,
        category: DiagnosticCategory::Message,
        message: "Wrap in parentheses",
    },
    DiagnosticMessage {
        code: 95195,
        category: DiagnosticCategory::Message,
        message: "Wrap all invalid decorator expressions in parentheses",
    },
    DiagnosticMessage {
        code: 95196,
        category: DiagnosticCategory::Message,
        message: "Add 'resolution-mode' import attribute",
    },
    DiagnosticMessage {
        code: 95197,
        category: DiagnosticCategory::Message,
        message: "Add 'resolution-mode' import attribute to all type-only imports that need it",
    },
];

/// Diagnostic message templates matching TypeScript exactly.
/// Use `format_message()` to fill in placeholders.
pub mod diagnostic_messages {
    pub const UNTERMINATED_STRING_LITERAL: &str = "Unterminated string literal.";
    pub const IDENTIFIER_EXPECTED: &str = "Identifier expected.";
    pub const EXPECTED: &str = "'{0}' expected.";
    pub const A_FILE_CANNOT_HAVE_A_REFERENCE_TO_ITSELF: &str =
        "A file cannot have a reference to itself.";
    pub const THE_PARSER_EXPECTED_TO_FIND_A_TO_MATCH_THE_TOKEN_HERE: &str =
        "The parser expected to find a '{1}' to match the '{0}' token here.";
    pub const TRAILING_COMMA_NOT_ALLOWED: &str = "Trailing comma not allowed.";
    pub const EXPECTED_2: &str = "'*/' expected.";
    pub const AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT: &str =
        "An element access expression should take an argument.";
    pub const UNEXPECTED_TOKEN: &str = "Unexpected token.";
    pub const A_REST_PARAMETER_OR_BINDING_PATTERN_MAY_NOT_HAVE_A_TRAILING_COMMA: &str =
        "A rest parameter or binding pattern may not have a trailing comma.";
    pub const A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST: &str =
        "A rest parameter must be last in a parameter list.";
    pub const PARAMETER_CANNOT_HAVE_QUESTION_MARK_AND_INITIALIZER: &str =
        "Parameter cannot have question mark and initializer.";
    pub const A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER: &str =
        "A required parameter cannot follow an optional parameter.";
    pub const AN_INDEX_SIGNATURE_CANNOT_HAVE_A_REST_PARAMETER: &str =
        "An index signature cannot have a rest parameter.";
    pub const AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER: &str =
        "An index signature parameter cannot have an accessibility modifier.";
    pub const AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_A_QUESTION_MARK: &str =
        "An index signature parameter cannot have a question mark.";
    pub const AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER: &str =
        "An index signature parameter cannot have an initializer.";
    pub const AN_INDEX_SIGNATURE_MUST_HAVE_A_TYPE_ANNOTATION: &str =
        "An index signature must have a type annotation.";
    pub const AN_INDEX_SIGNATURE_PARAMETER_MUST_HAVE_A_TYPE_ANNOTATION: &str =
        "An index signature parameter must have a type annotation.";
    pub const READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE: &str =
        "'readonly' modifier can only appear on a property declaration or index signature.";
    pub const AN_INDEX_SIGNATURE_CANNOT_HAVE_A_TRAILING_COMMA: &str =
        "An index signature cannot have a trailing comma.";
    pub const ACCESSIBILITY_MODIFIER_ALREADY_SEEN: &str = "Accessibility modifier already seen.";
    pub const MODIFIER_MUST_PRECEDE_MODIFIER: &str = "'{0}' modifier must precede '{1}' modifier.";
    pub const MODIFIER_ALREADY_SEEN: &str = "'{0}' modifier already seen.";
    pub const MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND: &str =
        "'{0}' modifier cannot appear on class elements of this kind.";
    pub const SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS: &str =
        "'super' must be followed by an argument list or member access.";
    pub const ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES: &str =
        "Only ambient modules can use quoted names.";
    pub const STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: &str =
        "Statements are not allowed in ambient contexts.";
    pub const A_DECLARE_MODIFIER_CANNOT_BE_USED_IN_AN_ALREADY_AMBIENT_CONTEXT: &str =
        "A 'declare' modifier cannot be used in an already ambient context.";
    pub const INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: &str =
        "Initializers are not allowed in ambient contexts.";
    pub const MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT: &str =
        "'{0}' modifier cannot be used in an ambient context.";
    pub const MODIFIER_CANNOT_BE_USED_HERE: &str = "'{0}' modifier cannot be used here.";
    pub const MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT: &str =
        "'{0}' modifier cannot appear on a module or namespace element.";
    pub const TOP_LEVEL_DECLARATIONS_IN_D_TS_FILES_MUST_START_WITH_EITHER_A_DECLARE_OR_EXPORT:
        &str = "Top-level declarations in .d.ts files must start with either a 'declare' or 'export' modifier.";
    pub const A_REST_PARAMETER_CANNOT_BE_OPTIONAL: &str = "A rest parameter cannot be optional.";
    pub const A_REST_PARAMETER_CANNOT_HAVE_AN_INITIALIZER: &str =
        "A rest parameter cannot have an initializer.";
    pub const A_SET_ACCESSOR_MUST_HAVE_EXACTLY_ONE_PARAMETER: &str =
        "A 'set' accessor must have exactly one parameter.";
    pub const A_SET_ACCESSOR_CANNOT_HAVE_AN_OPTIONAL_PARAMETER: &str =
        "A 'set' accessor cannot have an optional parameter.";
    pub const A_SET_ACCESSOR_PARAMETER_CANNOT_HAVE_AN_INITIALIZER: &str =
        "A 'set' accessor parameter cannot have an initializer.";
    pub const A_SET_ACCESSOR_CANNOT_HAVE_REST_PARAMETER: &str =
        "A 'set' accessor cannot have rest parameter.";
    pub const A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS: &str =
        "A 'get' accessor cannot have parameters.";
    pub const TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER:
        &str = "Type '{0}' is not a valid async function return type in ES5 because it does not refer to a Promise-compatible constructor value.";
    pub const ACCESSORS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_5_AND_HIGHER: &str =
        "Accessors are only available when targeting ECMAScript 5 and higher.";
    pub const THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT:
        &str = "The return type of an async function must either be a valid promise or must not contain a callable 'then' member.";
    pub const A_PROMISE_MUST_HAVE_A_THEN_METHOD: &str = "A promise must have a 'then' method.";
    pub const THE_FIRST_PARAMETER_OF_THE_THEN_METHOD_OF_A_PROMISE_MUST_BE_A_CALLBACK: &str =
        "The first parameter of the 'then' method of a promise must be a callback.";
    pub const ENUM_MEMBER_MUST_HAVE_INITIALIZER: &str = "Enum member must have initializer.";
    pub const TYPE_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_THE_FULFILLMENT_CALLBACK_OF_ITS_OWN:
        &str = "Type is referenced directly or indirectly in the fulfillment callback of its own 'then' method.";
    pub const AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE: &str =
        "An export assignment cannot be used in a namespace.";
    pub const THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE:
        &str = "The return type of an async function or method must be the global Promise<T> type. Did you mean to write 'Promise<{0}>'?";
    pub const THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE_2:
        &str = "The return type of an async function or method must be the global Promise<T> type.";
    pub const IN_AMBIENT_ENUM_DECLARATIONS_MEMBER_INITIALIZER_MUST_BE_CONSTANT_EXPRESSION: &str =
        "In ambient enum declarations member initializer must be constant expression.";
    pub const UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED: &str =
        "Unexpected token. A constructor, method, accessor, or property was expected.";
    pub const UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES: &str =
        "Unexpected token. A type parameter name was expected without curly braces.";
    pub const MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER: &str =
        "'{0}' modifier cannot appear on a type member.";
    pub const MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE: &str =
        "'{0}' modifier cannot appear on an index signature.";
    pub const A_MODIFIER_CANNOT_BE_USED_WITH_AN_IMPORT_DECLARATION: &str =
        "A '{0}' modifier cannot be used with an import declaration.";
    pub const INVALID_REFERENCE_DIRECTIVE_SYNTAX: &str = "Invalid 'reference' directive syntax.";
    pub const MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION: &str =
        "'{0}' modifier cannot appear on a constructor declaration.";
    pub const MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER: &str =
        "'{0}' modifier cannot appear on a parameter.";
    pub const ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_IN_STATEMENT: &str =
        "Only a single variable declaration is allowed in a 'for...in' statement.";
    pub const TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION: &str =
        "Type parameters cannot appear on a constructor declaration.";
    pub const TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION: &str =
        "Type annotation cannot appear on a constructor declaration.";
    pub const AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS: &str =
        "An accessor cannot have type parameters.";
    pub const A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION: &str =
        "A 'set' accessor cannot have a return type annotation.";
    pub const AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER: &str =
        "An index signature must have exactly one parameter.";
    pub const LIST_CANNOT_BE_EMPTY: &str = "'{0}' list cannot be empty.";
    pub const TYPE_PARAMETER_LIST_CANNOT_BE_EMPTY: &str = "Type parameter list cannot be empty.";
    pub const TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY: &str = "Type argument list cannot be empty.";
    pub const INVALID_USE_OF_IN_STRICT_MODE: &str = "Invalid use of '{0}' in strict mode.";
    pub const WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE: &str =
        "'with' statements are not allowed in strict mode.";
    pub const DELETE_CANNOT_BE_CALLED_ON_AN_IDENTIFIER_IN_STRICT_MODE: &str =
        "'delete' cannot be called on an identifier in strict mode.";
    pub const FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF:
        &str = "'for await' loops are only allowed within async functions and at the top levels of modules.";
    pub const A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT: &str =
        "A 'continue' statement can only be used within an enclosing iteration statement.";
    pub const A_BREAK_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_OR_SWITCH_STATE:
        &str =
        "A 'break' statement can only be used within an enclosing iteration or switch statement.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_ASYNC: &str =
        "The left-hand side of a 'for...of' statement may not be 'async'.";
    pub const JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY: &str =
        "Jump target cannot cross function boundary.";
    pub const A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY: &str =
        "A 'return' statement can only be used within a function body.";
    pub const EXPRESSION_EXPECTED: &str = "Expression expected.";
    pub const TYPE_EXPECTED: &str = "Type expected.";
    pub const PRIVATE_FIELD_MUST_BE_DECLARED_IN_AN_ENCLOSING_CLASS: &str =
        "Private field '{0}' must be declared in an enclosing class.";
    pub const A_DEFAULT_CLAUSE_CANNOT_APPEAR_MORE_THAN_ONCE_IN_A_SWITCH_STATEMENT: &str =
        "A 'default' clause cannot appear more than once in a 'switch' statement.";
    pub const DUPLICATE_LABEL: &str = "Duplicate label '{0}'.";
    pub const A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMEN:
        &str =
        "A 'continue' statement can only jump to a label of an enclosing iteration statement.";
    pub const A_BREAK_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_STATEMENT: &str =
        "A 'break' statement can only jump to a label of an enclosing statement.";
    pub const AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME: &str =
        "An object literal cannot have multiple properties with the same name.";
    pub const AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_GET_SET_ACCESSORS_WITH_THE_SAME_NAME: &str =
        "An object literal cannot have multiple get/set accessors with the same name.";
    pub const AN_OBJECT_LITERAL_CANNOT_HAVE_PROPERTY_AND_ACCESSOR_WITH_THE_SAME_NAME: &str =
        "An object literal cannot have property and accessor with the same name.";
    pub const AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS: &str =
        "An export assignment cannot have modifiers.";
    pub const OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX: &str =
        "Octal literals are not allowed. Use the syntax '{0}'.";
    pub const VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY: &str =
        "Variable declaration list cannot be empty.";
    pub const DIGIT_EXPECTED: &str = "Digit expected.";
    pub const HEXADECIMAL_DIGIT_EXPECTED: &str = "Hexadecimal digit expected.";
    pub const UNEXPECTED_END_OF_TEXT: &str = "Unexpected end of text.";
    pub const INVALID_CHARACTER: &str = "Invalid character.";
    pub const DECLARATION_OR_STATEMENT_EXPECTED: &str = "Declaration or statement expected.";
    pub const STATEMENT_EXPECTED: &str = "Statement expected.";
    pub const CASE_OR_DEFAULT_EXPECTED: &str = "'case' or 'default' expected.";
    pub const PROPERTY_OR_SIGNATURE_EXPECTED: &str = "Property or signature expected.";
    pub const ENUM_MEMBER_EXPECTED: &str = "Enum member expected.";
    pub const VARIABLE_DECLARATION_EXPECTED: &str = "Variable declaration expected.";
    pub const ARGUMENT_EXPRESSION_EXPECTED: &str = "Argument expression expected.";
    pub const PROPERTY_ASSIGNMENT_EXPECTED: &str = "Property assignment expected.";
    pub const EXPRESSION_OR_COMMA_EXPECTED: &str = "Expression or comma expected.";
    pub const PARAMETER_DECLARATION_EXPECTED: &str = "Parameter declaration expected.";
    pub const TYPE_PARAMETER_DECLARATION_EXPECTED: &str = "Type parameter declaration expected.";
    pub const TYPE_ARGUMENT_EXPECTED: &str = "Type argument expected.";
    pub const STRING_LITERAL_EXPECTED: &str = "String literal expected.";
    pub const LINE_BREAK_NOT_PERMITTED_HERE: &str = "Line break not permitted here.";
    pub const OR_EXPECTED: &str = "'{' or ';' expected.";
    pub const OR_JSX_ELEMENT_EXPECTED: &str = "'{' or JSX element expected.";
    pub const DECLARATION_EXPECTED: &str = "Declaration expected.";
    pub const IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE: &str =
        "Import declarations in a namespace cannot reference a module.";
    pub const CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE: &str =
        "Cannot use imports, exports, or module augmentations when '--module' is 'none'.";
    pub const FILE_NAME_DIFFERS_FROM_ALREADY_INCLUDED_FILE_NAME_ONLY_IN_CASING: &str =
        "File name '{0}' differs from already included file name '{1}' only in casing.";
    pub const DECLARATIONS_MUST_BE_INITIALIZED: &str = "'{0}' declarations must be initialized.";
    pub const DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK: &str =
        "'{0}' declarations can only be declared inside a block.";
    pub const UNTERMINATED_TEMPLATE_LITERAL: &str = "Unterminated template literal.";
    pub const UNTERMINATED_REGULAR_EXPRESSION_LITERAL: &str =
        "Unterminated regular expression literal.";
    pub const AN_OBJECT_MEMBER_CANNOT_BE_DECLARED_OPTIONAL: &str =
        "An object member cannot be declared optional.";
    pub const A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY: &str =
        "A 'yield' expression is only allowed in a generator body.";
    pub const COMPUTED_PROPERTY_NAMES_ARE_NOT_ALLOWED_IN_ENUMS: &str =
        "Computed property names are not allowed in enums.";
    pub const A_COMPUTED_PROPERTY_NAME_IN_AN_AMBIENT_CONTEXT_MUST_REFER_TO_AN_EXPRESSION_WHOSE:
        &str = "A computed property name in an ambient context must refer to an expression whose type is a literal type or a 'unique symbol' type.";
    pub const A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE:
        &str = "A computed property name in a class property declaration must have a simple literal type or a 'unique symbol' type.";
    pub const A_COMPUTED_PROPERTY_NAME_IN_A_METHOD_OVERLOAD_MUST_REFER_TO_AN_EXPRESSION_WHOSE:
        &str = "A computed property name in a method overload must refer to an expression whose type is a literal type or a 'unique symbol' type.";
    pub const A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE:
        &str = "A computed property name in an interface must refer to an expression whose type is a literal type or a 'unique symbol' type.";
    pub const A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP:
        &str = "A computed property name in a type literal must refer to an expression whose type is a literal type or a 'unique symbol' type.";
    pub const A_COMMA_EXPRESSION_IS_NOT_ALLOWED_IN_A_COMPUTED_PROPERTY_NAME: &str =
        "A comma expression is not allowed in a computed property name.";
    pub const EXTENDS_CLAUSE_ALREADY_SEEN: &str = "'extends' clause already seen.";
    pub const EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE: &str =
        "'extends' clause must precede 'implements' clause.";
    pub const CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS: &str =
        "Classes can only extend a single class.";
    pub const IMPLEMENTS_CLAUSE_ALREADY_SEEN: &str = "'implements' clause already seen.";
    pub const INTERFACE_DECLARATION_CANNOT_HAVE_IMPLEMENTS_CLAUSE: &str =
        "Interface declaration cannot have 'implements' clause.";
    pub const BINARY_DIGIT_EXPECTED: &str = "Binary digit expected.";
    pub const OCTAL_DIGIT_EXPECTED: &str = "Octal digit expected.";
    pub const UNEXPECTED_TOKEN_EXPECTED: &str = "Unexpected token. '{' expected.";
    pub const PROPERTY_DESTRUCTURING_PATTERN_EXPECTED: &str =
        "Property destructuring pattern expected.";
    pub const ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED: &str =
        "Array element destructuring pattern expected.";
    pub const A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER: &str =
        "A destructuring declaration must have an initializer.";
    pub const AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS: &str =
        "An implementation cannot be declared in ambient contexts.";
    pub const MODIFIERS_CANNOT_APPEAR_HERE: &str = "Modifiers cannot appear here.";
    pub const MERGE_CONFLICT_MARKER_ENCOUNTERED: &str = "Merge conflict marker encountered.";
    pub const A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER: &str =
        "A rest element cannot have an initializer.";
    pub const A_PARAMETER_PROPERTY_MAY_NOT_BE_DECLARED_USING_A_BINDING_PATTERN: &str =
        "A parameter property may not be declared using a binding pattern.";
    pub const ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_OF_STATEMENT: &str =
        "Only a single variable declaration is allowed in a 'for...of' statement.";
    pub const THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER: &str =
        "The variable declaration of a 'for...in' statement cannot have an initializer.";
    pub const THE_VARIABLE_DECLARATION_OF_A_FOR_OF_STATEMENT_CANNOT_HAVE_AN_INITIALIZER: &str =
        "The variable declaration of a 'for...of' statement cannot have an initializer.";
    pub const AN_IMPORT_DECLARATION_CANNOT_HAVE_MODIFIERS: &str =
        "An import declaration cannot have modifiers.";
    pub const MODULE_HAS_NO_DEFAULT_EXPORT: &str = "Module '{0}' has no default export.";
    pub const AN_EXPORT_DECLARATION_CANNOT_HAVE_MODIFIERS: &str =
        "An export declaration cannot have modifiers.";
    pub const EXPORT_DECLARATIONS_ARE_NOT_PERMITTED_IN_A_NAMESPACE: &str =
        "Export declarations are not permitted in a namespace.";
    pub const EXPORT_DOES_NOT_RE_EXPORT_A_DEFAULT: &str =
        "'export *' does not re-export a default.";
    pub const CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED: &str =
        "Catch clause variable type annotation must be 'any' or 'unknown' if specified.";
    pub const CATCH_CLAUSE_VARIABLE_CANNOT_HAVE_AN_INITIALIZER: &str =
        "Catch clause variable cannot have an initializer.";
    pub const AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE: &str =
        "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.";
    pub const UNTERMINATED_UNICODE_ESCAPE_SEQUENCE: &str = "Unterminated Unicode escape sequence.";
    pub const LINE_TERMINATOR_NOT_PERMITTED_BEFORE_ARROW: &str =
        "Line terminator not permitted before arrow.";
    pub const IMPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN:
        &str = "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.";
    pub const EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN:
        &str = "Export assignment cannot be used when targeting ECMAScript modules. Consider using 'export default' or another module format instead.";
    pub const RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE: &str =
        "Re-exporting a type when '{0}' is enabled requires using 'export type'.";
    pub const DECORATORS_ARE_NOT_VALID_HERE: &str = "Decorators are not valid here.";
    pub const DECORATORS_CANNOT_BE_APPLIED_TO_MULTIPLE_GET_SET_ACCESSORS_OF_THE_SAME_NAME: &str =
        "Decorators cannot be applied to multiple get/set accessors of the same name.";
    pub const INVALID_OPTIONAL_CHAIN_FROM_NEW_EXPRESSION_DID_YOU_MEAN_TO_CALL: &str =
        "Invalid optional chain from new expression. Did you mean to call '{0}()'?";
    pub const CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT:
        &str = "Code contained in a class is evaluated in JavaScript's strict mode which does not allow this use of '{0}'. For more information, see https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Strict_mode.";
    pub const A_CLASS_DECLARATION_WITHOUT_THE_DEFAULT_MODIFIER_MUST_HAVE_A_NAME: &str =
        "A class declaration without the 'default' modifier must have a name.";
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE: &str =
        "Identifier expected. '{0}' is a reserved word in strict mode.";
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO:
        &str = "Identifier expected. '{0}' is a reserved word in strict mode. Class definitions are automatically in strict mode.";
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY:
        &str = "Identifier expected. '{0}' is a reserved word in strict mode. Modules are automatically in strict mode.";
    pub const INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE: &str =
        "Invalid use of '{0}'. Modules are automatically in strict mode.";
    pub const IDENTIFIER_EXPECTED_ESMODULE_IS_RESERVED_AS_AN_EXPORTED_MARKER_WHEN_TRANSFORMING:
        &str = "Identifier expected. '__esModule' is reserved as an exported marker when transforming ECMAScript modules.";
    pub const EXPORT_ASSIGNMENT_IS_NOT_SUPPORTED_WHEN_MODULE_FLAG_IS_SYSTEM: &str =
        "Export assignment is not supported when '--module' flag is 'system'.";
    pub const GENERATORS_ARE_NOT_ALLOWED_IN_AN_AMBIENT_CONTEXT: &str =
        "Generators are not allowed in an ambient context.";
    pub const AN_OVERLOAD_SIGNATURE_CANNOT_BE_DECLARED_AS_A_GENERATOR: &str =
        "An overload signature cannot be declared as a generator.";
    pub const TAG_ALREADY_SPECIFIED: &str = "'{0}' tag already specified.";
    pub const SIGNATURE_MUST_BE_A_TYPE_PREDICATE: &str =
        "Signature '{0}' must be a type predicate.";
    pub const CANNOT_FIND_PARAMETER: &str = "Cannot find parameter '{0}'.";
    pub const TYPE_PREDICATE_IS_NOT_ASSIGNABLE_TO: &str =
        "Type predicate '{0}' is not assignable to '{1}'.";
    pub const PARAMETER_IS_NOT_IN_THE_SAME_POSITION_AS_PARAMETER: &str =
        "Parameter '{0}' is not in the same position as parameter '{1}'.";
    pub const A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO:
        &str =
        "A type predicate is only allowed in return type position for functions and methods.";
    pub const A_TYPE_PREDICATE_CANNOT_REFERENCE_A_REST_PARAMETER: &str =
        "A type predicate cannot reference a rest parameter.";
    pub const A_TYPE_PREDICATE_CANNOT_REFERENCE_ELEMENT_IN_A_BINDING_PATTERN: &str =
        "A type predicate cannot reference element '{0}' in a binding pattern.";
    pub const AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION: &str =
        "An export assignment must be at the top level of a file or module declaration.";
    pub const AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE:
        &str = "An import declaration can only be used at the top level of a namespace or module.";
    pub const AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE:
        &str = "An export declaration can only be used at the top level of a namespace or module.";
    pub const AN_AMBIENT_MODULE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_IN_A_FILE: &str =
        "An ambient module declaration is only allowed at the top level in a file.";
    pub const A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODUL:
        &str = "A namespace declaration is only allowed at the top level of a namespace or module.";
    pub const THE_RETURN_TYPE_OF_A_PROPERTY_DECORATOR_FUNCTION_MUST_BE_EITHER_VOID_OR_ANY: &str =
        "The return type of a property decorator function must be either 'void' or 'any'.";
    pub const THE_RETURN_TYPE_OF_A_PARAMETER_DECORATOR_FUNCTION_MUST_BE_EITHER_VOID_OR_ANY: &str =
        "The return type of a parameter decorator function must be either 'void' or 'any'.";
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION: &str =
        "Unable to resolve signature of class decorator when called as an expression.";
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_PARAMETER_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION:
        &str = "Unable to resolve signature of parameter decorator when called as an expression.";
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION: &str =
        "Unable to resolve signature of property decorator when called as an expression.";
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION: &str =
        "Unable to resolve signature of method decorator when called as an expression.";
    pub const ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION: &str =
        "'abstract' modifier can only appear on a class, method, or property declaration.";
    pub const MODIFIER_CANNOT_BE_USED_WITH_MODIFIER: &str =
        "'{0}' modifier cannot be used with '{1}' modifier.";
    pub const ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS: &str =
        "Abstract methods can only appear within an abstract class.";
    pub const METHOD_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT: &str =
        "Method '{0}' cannot have an implementation because it is marked abstract.";
    pub const AN_INTERFACE_PROPERTY_CANNOT_HAVE_AN_INITIALIZER: &str =
        "An interface property cannot have an initializer.";
    pub const A_TYPE_LITERAL_PROPERTY_CANNOT_HAVE_AN_INITIALIZER: &str =
        "A type literal property cannot have an initializer.";
    pub const A_CLASS_MEMBER_CANNOT_HAVE_THE_KEYWORD: &str =
        "A class member cannot have the '{0}' keyword.";
    pub const A_DECORATOR_CAN_ONLY_DECORATE_A_METHOD_IMPLEMENTATION_NOT_AN_OVERLOAD: &str =
        "A decorator can only decorate a method implementation, not an overload.";
    pub const FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN:
        &str =
        "Function declarations are not allowed inside blocks in strict mode when targeting 'ES5'.";
    pub const FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_2:
        &str = "Function declarations are not allowed inside blocks in strict mode when targeting 'ES5'. Class definitions are automatically in strict mode.";
    pub const FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_3:
        &str = "Function declarations are not allowed inside blocks in strict mode when targeting 'ES5'. Modules are automatically in strict mode.";
    pub const ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS: &str =
        "Abstract properties can only appear within an abstract class.";
    pub const A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR:
        &str = "A 'const' initializer in an ambient context must be a string or numeric literal or literal enum reference.";
    pub const A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT: &str =
        "A definite assignment assertion '!' is not permitted in this context.";
    pub const A_REQUIRED_ELEMENT_CANNOT_FOLLOW_AN_OPTIONAL_ELEMENT: &str =
        "A required element cannot follow an optional element.";
    pub const A_DEFAULT_EXPORT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION: &str =
        "A default export must be at the top level of a file or module declaration.";
    pub const MODULE_CAN_ONLY_BE_DEFAULT_IMPORTED_USING_THE_FLAG: &str =
        "Module '{0}' can only be default-imported using the '{1}' flag";
    pub const KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS: &str =
        "Keywords cannot contain escape characters.";
    pub const ALREADY_INCLUDED_FILE_NAME_DIFFERS_FROM_FILE_NAME_ONLY_IN_CASING: &str =
        "Already included file name '{0}' differs from file name '{1}' only in casing.";
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE: &str =
        "Identifier expected. '{0}' is a reserved word at the top-level of a module.";
    pub const DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS: &str =
        "Declarations with initializers cannot also have definite assignment assertions.";
    pub const DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS:
        &str = "Declarations with definite assignment assertions must also have type annotations.";
    pub const A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT: &str =
        "A rest element cannot follow another rest element.";
    pub const AN_OPTIONAL_ELEMENT_CANNOT_FOLLOW_A_REST_ELEMENT: &str =
        "An optional element cannot follow a rest element.";
    pub const PROPERTY_CANNOT_HAVE_AN_INITIALIZER_BECAUSE_IT_IS_MARKED_ABSTRACT: &str =
        "Property '{0}' cannot have an initializer because it is marked abstract.";
    pub const AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT:
        &str = "An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.";
    pub const CANNOT_USE_EXPORT_IMPORT_ON_A_TYPE_OR_TYPE_ONLY_NAMESPACE_WHEN_IS_ENABLED: &str =
        "Cannot use 'export import' on a type or type-only namespace when '{0}' is enabled.";
    pub const DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE: &str =
        "Decorator function return type '{0}' is not assignable to type '{1}'.";
    pub const DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY: &str =
        "Decorator function return type is '{0}' but is expected to be 'void' or 'any'.";
    pub const A_TYPE_REFERENCED_IN_A_DECORATED_SIGNATURE_MUST_BE_IMPORTED_WITH_IMPORT_TYPE_OR:
        &str = "A type referenced in a decorated signature must be imported with 'import type' or a namespace import when 'isolatedModules' and 'emitDecoratorMetadata' are enabled.";
    pub const MODIFIER_CANNOT_APPEAR_ON_A_TYPE_PARAMETER: &str =
        "'{0}' modifier cannot appear on a type parameter";
    pub const MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_CLASS_INTERFACE_OR_TYPE_ALIAS:
        &str =
        "'{0}' modifier can only appear on a type parameter of a class, interface or type alias";
    pub const ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION: &str =
        "'accessor' modifier can only appear on a property declaration.";
    pub const AN_ACCESSOR_PROPERTY_CANNOT_BE_DECLARED_OPTIONAL: &str =
        "An 'accessor' property cannot be declared optional.";
    pub const MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_FUNCTION_METHOD_OR_CLASS: &str =
        "'{0}' modifier can only appear on a type parameter of a function, method or class";
    pub const THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS: &str =
        "The runtime will invoke the decorator with {1} arguments, but the decorator expects {0}.";
    pub const THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS_A:
        &str = "The runtime will invoke the decorator with {1} arguments, but the decorator expects at least {0}.";
    pub const NAMESPACES_ARE_NOT_ALLOWED_IN_GLOBAL_SCRIPT_FILES_WHEN_IS_ENABLED_IF_THIS_FILE_I:
        &str = "Namespaces are not allowed in global script files when '{0}' is enabled. If this file is not intended to be a global script, set 'moduleDetection' to 'force' or add an empty 'export {}' statement.";
    pub const CANNOT_ACCESS_FROM_ANOTHER_FILE_WITHOUT_QUALIFICATION_WHEN_IS_ENABLED_USE_INSTEA:
        &str = "Cannot access '{0}' from another file without qualification when '{1}' is enabled. Use '{2}' instead.";
    pub const AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE:
        &str = "An 'export =' declaration must reference a value when 'verbatimModuleSyntax' is enabled, but '{0}' only refers to a type.";
    pub const AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E:
        &str = "An 'export =' declaration must reference a real value when 'verbatimModuleSyntax' is enabled, but '{0}' resolves to a type-only declaration.";
    pub const AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU:
        &str = "An 'export default' must reference a value when 'verbatimModuleSyntax' is enabled, but '{0}' only refers to a type.";
    pub const AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL:
        &str = "An 'export default' must reference a real value when 'verbatimModuleSyntax' is enabled, but '{0}' resolves to a type-only declaration.";
    pub const ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT:
        &str = "ECMAScript imports and exports cannot be written in a CommonJS file under 'verbatimModuleSyntax'.";
    pub const A_TOP_LEVEL_EXPORT_MODIFIER_CANNOT_BE_USED_ON_VALUE_DECLARATIONS_IN_A_COMMONJS_M:
        &str = "A top-level 'export' modifier cannot be used on value declarations in a CommonJS module when 'verbatimModuleSyntax' is enabled.";
    pub const AN_IMPORT_ALIAS_CANNOT_RESOLVE_TO_A_TYPE_OR_TYPE_ONLY_DECLARATION_WHEN_VERBATIMM:
        &str = "An import alias cannot resolve to a type or type-only declaration when 'verbatimModuleSyntax' is enabled.";
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BE:
        &str = "'{0}' resolves to a type-only declaration and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'import type' where '{0}' is imported.";
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BE_2:
        &str = "'{0}' resolves to a type-only declaration and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'export type { {0} as default }'.";
    pub const RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING:
        &str = "'{0}' resolves to a type and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'import type' where '{0}' is imported.";
    pub const RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_2:
        &str = "'{0}' resolves to a type and must be marked type-only in this file before re-exporting when '{1}' is enabled. Consider using 'export type { {0} as default }'.";
    pub const ECMASCRIPT_MODULE_SYNTAX_IS_NOT_ALLOWED_IN_A_COMMONJS_MODULE_WHEN_MODULE_IS_SET:
        &str = "ECMAScript module syntax is not allowed in a CommonJS module when 'module' is set to 'preserve'.";
    pub const THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED: &str =
        "This syntax is not allowed when 'erasableSyntaxOnly' is enabled.";
    pub const ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2:
        &str = "ECMAScript imports and exports cannot be written in a CommonJS file under 'verbatimModuleSyntax'. Adjust the 'type' field in the nearest 'package.json' to make this file an ECMAScript module, or adjust your 'verbatimModuleSyntax', 'module', and 'moduleResolution' settings in TypeScript.";
    pub const WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_AN_ASYNC_FUNCTION_BLOCK: &str =
        "'with' statements are not allowed in an async function block.";
    pub const AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS:
        &str = "'await' expressions are only allowed within async functions and at the top levels of modules.";
    pub const THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_AND_CANNOT_USE_AWAIT_AT_THE_TOP_LEVEL: &str =
        "The current file is a CommonJS module and cannot use 'await' at the top level.";
    pub const DID_YOU_MEAN_TO_USE_A_AN_CAN_ONLY_FOLLOW_A_PROPERTY_NAME_WHEN_THE_CONTAINING_OBJ:
        &str = "Did you mean to use a ':'? An '=' can only follow a property name when the containing object literal is part of a destructuring pattern.";
    pub const THE_BODY_OF_AN_IF_STATEMENT_CANNOT_BE_THE_EMPTY_STATEMENT: &str =
        "The body of an 'if' statement cannot be the empty statement.";
    pub const GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_MODULE_FILES: &str =
        "Global module exports may only appear in module files.";
    pub const GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_DECLARATION_FILES: &str =
        "Global module exports may only appear in declaration files.";
    pub const GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_AT_TOP_LEVEL: &str =
        "Global module exports may only appear at top level.";
    pub const A_PARAMETER_PROPERTY_CANNOT_BE_DECLARED_USING_A_REST_PARAMETER: &str =
        "A parameter property cannot be declared using a rest parameter.";
    pub const AN_ABSTRACT_ACCESSOR_CANNOT_HAVE_AN_IMPLEMENTATION: &str =
        "An abstract accessor cannot have an implementation.";
    pub const A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE: &str =
        "A default export can only be used in an ECMAScript-style module.";
    pub const TYPE_OF_AWAIT_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLA:
        &str = "Type of 'await' operand must either be a valid promise or must not contain a callable 'then' member.";
    pub const TYPE_OF_YIELD_OPERAND_IN_AN_ASYNC_GENERATOR_MUST_EITHER_BE_A_VALID_PROMISE_OR_MU:
        &str = "Type of 'yield' operand in an async generator must either be a valid promise or must not contain a callable 'then' member.";
    pub const TYPE_OF_ITERATED_ELEMENTS_OF_A_YIELD_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_M:
        &str = "Type of iterated elements of a 'yield*' operand must either be a valid promise or must not contain a callable 'then' member.";
    pub const DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022:
        &str = "Dynamic imports are only supported when the '--module' flag is set to 'es2020', 'es2022', 'esnext', 'commonjs', 'amd', 'system', 'umd', 'node16', 'node18', 'node20', or 'nodenext'.";
    pub const DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO:
        &str = "Dynamic imports only support a second argument when the '--module' option is set to 'esnext', 'node16', 'node18', 'node20', 'nodenext', or 'preserve'.";
    pub const ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT: &str =
        "Argument of dynamic import cannot be spread element.";
    pub const THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR:
        &str = "This use of 'import' is invalid. 'import()' calls can be written, but they must have parentheses and cannot have type arguments.";
    pub const STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED: &str =
        "String literal with double quotes expected.";
    pub const PROPERTY_VALUE_CAN_ONLY_BE_STRING_LITERAL_NUMERIC_LITERAL_TRUE_FALSE_NULL_OBJECT:
        &str = "Property value can only be string literal, numeric literal, 'true', 'false', 'null', object literal or array literal.";
    pub const ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT:
        &str = "'{0}' accepts too few arguments to be used as a decorator here. Did you mean to call it first and write '@{0}()'?";
    pub const A_PROPERTY_OF_AN_INTERFACE_OR_TYPE_LITERAL_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MU:
        &str = "A property of an interface or type literal whose type is a 'unique symbol' type must be 'readonly'.";
    pub const A_PROPERTY_OF_A_CLASS_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_BOTH_STATIC_AND:
        &str = "A property of a class whose type is a 'unique symbol' type must be both 'static' and 'readonly'.";
    pub const A_VARIABLE_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_CONST: &str =
        "A variable whose type is a 'unique symbol' type must be 'const'.";
    pub const UNIQUE_SYMBOL_TYPES_MAY_NOT_BE_USED_ON_A_VARIABLE_DECLARATION_WITH_A_BINDING_NAM:
        &str =
        "'unique symbol' types may not be used on a variable declaration with a binding name.";
    pub const UNIQUE_SYMBOL_TYPES_ARE_ONLY_ALLOWED_ON_VARIABLES_IN_A_VARIABLE_STATEMENT: &str =
        "'unique symbol' types are only allowed on variables in a variable statement.";
    pub const UNIQUE_SYMBOL_TYPES_ARE_NOT_ALLOWED_HERE: &str =
        "'unique symbol' types are not allowed here.";
    pub const AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI:
        &str = "An index signature parameter type cannot be a literal type or generic type. Consider using a mapped object type instead.";
    pub const INFER_DECLARATIONS_ARE_ONLY_PERMITTED_IN_THE_EXTENDS_CLAUSE_OF_A_CONDITIONAL_TYP:
        &str =
        "'infer' declarations are only permitted in the 'extends' clause of a conditional type.";
    pub const MODULE_DOES_NOT_REFER_TO_A_VALUE_BUT_IS_USED_AS_A_VALUE_HERE: &str =
        "Module '{0}' does not refer to a value, but is used as a value here.";
    pub const MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I:
        &str = "Module '{0}' does not refer to a type, but is used as a type here. Did you mean 'typeof import('{0}')'?";
    pub const CLASS_CONSTRUCTOR_MAY_NOT_BE_AN_ACCESSOR: &str =
        "Class constructor may not be an accessor.";
    pub const THE_IMPORT_META_META_PROPERTY_IS_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_ES2020_E:
        &str = "The 'import.meta' meta-property is only allowed when the '--module' option is 'es2020', 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', or 'nodenext'.";
    pub const A_LABEL_IS_NOT_ALLOWED_HERE: &str = "'A label is not allowed here.";
    pub const AN_EXPRESSION_OF_TYPE_VOID_CANNOT_BE_TESTED_FOR_TRUTHINESS: &str =
        "An expression of type 'void' cannot be tested for truthiness.";
    pub const THIS_PARAMETER_IS_NOT_ALLOWED_WITH_USE_STRICT_DIRECTIVE: &str =
        "This parameter is not allowed with 'use strict' directive.";
    pub const USE_STRICT_DIRECTIVE_CANNOT_BE_USED_WITH_NON_SIMPLE_PARAMETER_LIST: &str =
        "'use strict' directive cannot be used with non-simple parameter list.";
    pub const NON_SIMPLE_PARAMETER_DECLARED_HERE: &str = "Non-simple parameter declared here.";
    pub const USE_STRICT_DIRECTIVE_USED_HERE: &str = "'use strict' directive used here.";
    pub const PRINT_THE_FINAL_CONFIGURATION_INSTEAD_OF_BUILDING: &str =
        "Print the final configuration instead of building.";
    pub const AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL: &str =
        "An identifier or keyword cannot immediately follow a numeric literal.";
    pub const A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION: &str =
        "A bigint literal cannot use exponential notation.";
    pub const A_BIGINT_LITERAL_MUST_BE_AN_INTEGER: &str = "A bigint literal must be an integer.";
    pub const READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES: &str =
        "'readonly' type modifier is only permitted on array and tuple literal types.";
    pub const A_CONST_ASSERTION_CAN_ONLY_BE_APPLIED_TO_REFERENCES_TO_ENUM_MEMBERS_OR_STRING_NU:
        &str = "A 'const' assertion can only be applied to references to enum members, or string, number, boolean, array, or object literals.";
    pub const DID_YOU_MEAN_TO_MARK_THIS_FUNCTION_AS_ASYNC: &str =
        "Did you mean to mark this function as 'async'?";
    pub const AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR: &str =
        "An enum member name must be followed by a ',', '=', or '}'.";
    pub const TAGGED_TEMPLATE_EXPRESSIONS_ARE_NOT_PERMITTED_IN_AN_OPTIONAL_CHAIN: &str =
        "Tagged template expressions are not permitted in an optional chain.";
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE: &str =
        "Identifier expected. '{0}' is a reserved word that cannot be used here.";
    pub const TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE: &str =
        "Type '{0}' does not satisfy the expected type '{1}'.";
    pub const CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE: &str =
        "'{0}' cannot be used as a value because it was imported using 'import type'.";
    pub const CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE: &str =
        "'{0}' cannot be used as a value because it was exported using 'export type'.";
    pub const A_TYPE_ONLY_IMPORT_CAN_SPECIFY_A_DEFAULT_IMPORT_OR_NAMED_BINDINGS_BUT_NOT_BOTH: &str =
        "A type-only import can specify a default import or named bindings, but not both.";
    pub const CONVERT_TO_TYPE_ONLY_EXPORT: &str = "Convert to type-only export";
    pub const CONVERT_ALL_RE_EXPORTED_TYPES_TO_TYPE_ONLY_EXPORTS: &str =
        "Convert all re-exported types to type-only exports";
    pub const SPLIT_INTO_TWO_SEPARATE_IMPORT_DECLARATIONS: &str =
        "Split into two separate import declarations";
    pub const SPLIT_ALL_INVALID_TYPE_ONLY_IMPORTS: &str = "Split all invalid type-only imports";
    pub const CLASS_CONSTRUCTOR_MAY_NOT_BE_A_GENERATOR: &str =
        "Class constructor may not be a generator.";
    pub const DID_YOU_MEAN: &str = "Did you mean '{0}'?";
    pub const AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS:
        &str = "'await' expressions are only allowed at the top level of a file when that file is a module, but this file has no imports or exports. Consider adding an empty 'export {}' to make this file a module.";
    pub const WAS_IMPORTED_HERE: &str = "'{0}' was imported here.";
    pub const WAS_EXPORTED_HERE: &str = "'{0}' was exported here.";
    pub const TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES:
        &str = "Top-level 'await' expressions are only allowed when the 'module' option is set to 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', 'nodenext', or 'preserve', and the 'target' option is set to 'es2017' or higher.";
    pub const AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_EXPORTED_USING_EXPORT_TY:
        &str =
        "An import alias cannot reference a declaration that was exported using 'export type'.";
    pub const AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_IMPORTED_USING_IMPORT_TY:
        &str =
        "An import alias cannot reference a declaration that was imported using 'import type'.";
    pub const UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE: &str =
        "Unexpected token. Did you mean `{'}'}` or `&rbrace;`?";
    pub const UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT: &str =
        "Unexpected token. Did you mean `{'>'}` or `&gt;`?";
    pub const FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE: &str =
        "Function type notation must be parenthesized when used in a union type.";
    pub const CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE: &str =
        "Constructor type notation must be parenthesized when used in a union type.";
    pub const FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYPE: &str =
        "Function type notation must be parenthesized when used in an intersection type.";
    pub const CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYP:
        &str = "Constructor type notation must be parenthesized when used in an intersection type.";
    pub const IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME: &str =
        "'{0}' is not allowed as a variable declaration name.";
    pub const IS_NOT_ALLOWED_AS_A_PARAMETER_NAME: &str =
        "'{0}' is not allowed as a parameter name.";
    pub const AN_IMPORT_ALIAS_CANNOT_USE_IMPORT_TYPE: &str =
        "An import alias cannot use 'import type'";
    pub const IMPORTED_VIA_FROM_FILE: &str = "Imported via {0} from file '{1}'";
    pub const IMPORTED_VIA_FROM_FILE_WITH_PACKAGEID: &str =
        "Imported via {0} from file '{1}' with packageId '{2}'";
    pub const IMPORTED_VIA_FROM_FILE_TO_IMPORT_IMPORTHELPERS_AS_SPECIFIED_IN_COMPILEROPTIONS: &str =
        "Imported via {0} from file '{1}' to import 'importHelpers' as specified in compilerOptions";
    pub const IMPORTED_VIA_FROM_FILE_WITH_PACKAGEID_TO_IMPORT_IMPORTHELPERS_AS_SPECIFIED_IN_CO:
        &str = "Imported via {0} from file '{1}' with packageId '{2}' to import 'importHelpers' as specified in compilerOptions";
    pub const IMPORTED_VIA_FROM_FILE_TO_IMPORT_JSX_AND_JSXS_FACTORY_FUNCTIONS: &str =
        "Imported via {0} from file '{1}' to import 'jsx' and 'jsxs' factory functions";
    pub const IMPORTED_VIA_FROM_FILE_WITH_PACKAGEID_TO_IMPORT_JSX_AND_JSXS_FACTORY_FUNCTIONS: &str = "Imported via {0} from file '{1}' with packageId '{2}' to import 'jsx' and 'jsxs' factory functions";
    pub const FILE_IS_INCLUDED_VIA_IMPORT_HERE: &str = "File is included via import here.";
    pub const REFERENCED_VIA_FROM_FILE: &str = "Referenced via '{0}' from file '{1}'";
    pub const FILE_IS_INCLUDED_VIA_REFERENCE_HERE: &str = "File is included via reference here.";
    pub const TYPE_LIBRARY_REFERENCED_VIA_FROM_FILE: &str =
        "Type library referenced via '{0}' from file '{1}'";
    pub const TYPE_LIBRARY_REFERENCED_VIA_FROM_FILE_WITH_PACKAGEID: &str =
        "Type library referenced via '{0}' from file '{1}' with packageId '{2}'";
    pub const FILE_IS_INCLUDED_VIA_TYPE_LIBRARY_REFERENCE_HERE: &str =
        "File is included via type library reference here.";
    pub const LIBRARY_REFERENCED_VIA_FROM_FILE: &str =
        "Library referenced via '{0}' from file '{1}'";
    pub const FILE_IS_INCLUDED_VIA_LIBRARY_REFERENCE_HERE: &str =
        "File is included via library reference here.";
    pub const MATCHED_BY_INCLUDE_PATTERN_IN: &str = "Matched by include pattern '{0}' in '{1}'";
    pub const FILE_IS_MATCHED_BY_INCLUDE_PATTERN_SPECIFIED_HERE: &str =
        "File is matched by include pattern specified here.";
    pub const PART_OF_FILES_LIST_IN_TSCONFIG_JSON: &str = "Part of 'files' list in tsconfig.json";
    pub const FILE_IS_MATCHED_BY_FILES_LIST_SPECIFIED_HERE: &str =
        "File is matched by 'files' list specified here.";
    pub const OUTPUT_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_SPECIFIED: &str =
        "Output from referenced project '{0}' included because '{1}' specified";
    pub const OUTPUT_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_MODULE_IS_SPECIFIED_AS_NONE: &str =
        "Output from referenced project '{0}' included because '--module' is specified as 'none'";
    pub const FILE_IS_OUTPUT_FROM_REFERENCED_PROJECT_SPECIFIED_HERE: &str =
        "File is output from referenced project specified here.";
    pub const SOURCE_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_SPECIFIED: &str =
        "Source from referenced project '{0}' included because '{1}' specified";
    pub const SOURCE_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_MODULE_IS_SPECIFIED_AS_NONE: &str =
        "Source from referenced project '{0}' included because '--module' is specified as 'none'";
    pub const FILE_IS_SOURCE_FROM_REFERENCED_PROJECT_SPECIFIED_HERE: &str =
        "File is source from referenced project specified here.";
    pub const ENTRY_POINT_OF_TYPE_LIBRARY_SPECIFIED_IN_COMPILEROPTIONS: &str =
        "Entry point of type library '{0}' specified in compilerOptions";
    pub const ENTRY_POINT_OF_TYPE_LIBRARY_SPECIFIED_IN_COMPILEROPTIONS_WITH_PACKAGEID: &str =
        "Entry point of type library '{0}' specified in compilerOptions with packageId '{1}'";
    pub const FILE_IS_ENTRY_POINT_OF_TYPE_LIBRARY_SPECIFIED_HERE: &str =
        "File is entry point of type library specified here.";
    pub const ENTRY_POINT_FOR_IMPLICIT_TYPE_LIBRARY: &str =
        "Entry point for implicit type library '{0}'";
    pub const ENTRY_POINT_FOR_IMPLICIT_TYPE_LIBRARY_WITH_PACKAGEID: &str =
        "Entry point for implicit type library '{0}' with packageId '{1}'";
    pub const LIBRARY_SPECIFIED_IN_COMPILEROPTIONS: &str =
        "Library '{0}' specified in compilerOptions";
    pub const FILE_IS_LIBRARY_SPECIFIED_HERE: &str = "File is library specified here.";
    pub const DEFAULT_LIBRARY: &str = "Default library";
    pub const DEFAULT_LIBRARY_FOR_TARGET: &str = "Default library for target '{0}'";
    pub const FILE_IS_DEFAULT_LIBRARY_FOR_TARGET_SPECIFIED_HERE: &str =
        "File is default library for target specified here.";
    pub const ROOT_FILE_SPECIFIED_FOR_COMPILATION: &str = "Root file specified for compilation";
    pub const FILE_IS_OUTPUT_OF_PROJECT_REFERENCE_SOURCE: &str =
        "File is output of project reference source '{0}'";
    pub const FILE_REDIRECTS_TO_FILE: &str = "File redirects to file '{0}'";
    pub const THE_FILE_IS_IN_THE_PROGRAM_BECAUSE: &str = "The file is in the program because:";
    pub const FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A:
        &str = "'for await' loops are only allowed at the top level of a file when that file is a module, but this file has no imports or exports. Consider adding an empty 'export {}' to make this file a module.";
    pub const TOP_LEVEL_FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES20:
        &str = "Top-level 'for await' loops are only allowed when the 'module' option is set to 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', 'nodenext', or 'preserve', and the 'target' option is set to 'es2017' or higher.";
    pub const NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS: &str =
        "Neither decorators nor modifiers may be applied to 'this' parameters.";
    pub const UNEXPECTED_KEYWORD_OR_IDENTIFIER: &str = "Unexpected keyword or identifier.";
    pub const UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN: &str =
        "Unknown keyword or identifier. Did you mean '{0}'?";
    pub const DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS: &str =
        "Decorators must precede the name and all keywords of property declarations.";
    pub const NAMESPACE_MUST_BE_GIVEN_A_NAME: &str = "Namespace must be given a name.";
    pub const INTERFACE_MUST_BE_GIVEN_A_NAME: &str = "Interface must be given a name.";
    pub const TYPE_ALIAS_MUST_BE_GIVEN_A_NAME: &str = "Type alias must be given a name.";
    pub const VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION: &str =
        "Variable declaration not allowed at this location.";
    pub const CANNOT_START_A_FUNCTION_CALL_IN_A_TYPE_ANNOTATION: &str =
        "Cannot start a function call in a type annotation.";
    pub const EXPECTED_FOR_PROPERTY_INITIALIZER: &str = "Expected '=' for property initializer.";
    pub const MODULE_DECLARATION_NAMES_MAY_ONLY_USE_OR_QUOTED_STRINGS: &str =
        "Module declaration names may only use ' or \" quoted strings.";
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE:
        &str = "'{0}' resolves to a type-only declaration and must be re-exported using a type-only re-export when '{1}' is enabled.";
    pub const PRESERVE_UNUSED_IMPORTED_VALUES_IN_THE_JAVASCRIPT_OUTPUT_THAT_WOULD_OTHERWISE_BE:
        &str =
        "Preserve unused imported values in the JavaScript output that would otherwise be removed.";
    pub const DYNAMIC_IMPORTS_CAN_ONLY_ACCEPT_A_MODULE_SPECIFIER_AND_AN_OPTIONAL_SET_OF_ATTRIB:
        &str = "Dynamic imports can only accept a module specifier and an optional set of attributes as arguments";
    pub const PRIVATE_IDENTIFIERS_ARE_ONLY_ALLOWED_IN_CLASS_BODIES_AND_MAY_ONLY_BE_USED_AS_PAR:
        &str = "Private identifiers are only allowed in class bodies and may only be used as part of a class member declaration, property access, or on the left-hand-side of an 'in' expression";
    pub const RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT: &str =
        "`resolution-mode` should be either `require` or `import`.";
    pub const RESOLUTION_MODE_CAN_ONLY_BE_SET_FOR_TYPE_ONLY_IMPORTS: &str =
        "`resolution-mode` can only be set for type-only imports.";
    pub const RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ASSERTIONS: &str =
        "`resolution-mode` is the only valid key for type import assertions.";
    pub const TYPE_IMPORT_ASSERTIONS_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM:
        &str = "Type import assertions should have exactly one key - `resolution-mode` - with value `import` or `require`.";
    pub const MATCHED_BY_DEFAULT_INCLUDE_PATTERN: &str =
        "Matched by default include pattern '**/*'";
    pub const FILE_IS_ECMASCRIPT_MODULE_BECAUSE_HAS_FIELD_TYPE_WITH_VALUE_MODULE: &str =
        "File is ECMAScript module because '{0}' has field \"type\" with value \"module\"";
    pub const FILE_IS_COMMONJS_MODULE_BECAUSE_HAS_FIELD_TYPE_WHOSE_VALUE_IS_NOT_MODULE: &str =
        "File is CommonJS module because '{0}' has field \"type\" whose value is not \"module\"";
    pub const FILE_IS_COMMONJS_MODULE_BECAUSE_DOES_NOT_HAVE_FIELD_TYPE: &str =
        "File is CommonJS module because '{0}' does not have field \"type\"";
    pub const FILE_IS_COMMONJS_MODULE_BECAUSE_PACKAGE_JSON_WAS_NOT_FOUND: &str =
        "File is CommonJS module because 'package.json' was not found";
    pub const RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ATTRIBUTES: &str =
        "'resolution-mode' is the only valid key for type import attributes.";
    pub const TYPE_IMPORT_ATTRIBUTES_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM:
        &str = "Type import attributes should have exactly one key - 'resolution-mode' - with value 'import' or 'require'.";
    pub const THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM:
        &str = "The 'import.meta' meta-property is not allowed in files which will build into CommonJS output.";
    pub const MODULE_CANNOT_BE_IMPORTED_USING_THIS_CONSTRUCT_THE_SPECIFIER_ONLY_RESOLVES_TO_AN:
        &str = "Module '{0}' cannot be imported using this construct. The specifier only resolves to an ES module, which cannot be imported with 'require'. Use an ECMAScript import instead.";
    pub const CATCH_OR_FINALLY_EXPECTED: &str = "'catch' or 'finally' expected.";
    pub const AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE: &str =
        "An import declaration can only be used at the top level of a module.";
    pub const AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE: &str =
        "An export declaration can only be used at the top level of a module.";
    pub const CONTROL_WHAT_METHOD_IS_USED_TO_DETECT_MODULE_FORMAT_JS_FILES: &str =
        "Control what method is used to detect module-format JS files.";
    pub const AUTO_TREAT_FILES_WITH_IMPORTS_EXPORTS_IMPORT_META_JSX_WITH_JSX_REACT_JSX_OR_ESM:
        &str = "\"auto\": Treat files with imports, exports, import.meta, jsx (with jsx: react-jsx), or esm format (with module: node16+) as modules.";
    pub const AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS: &str =
        "An instantiation expression cannot be followed by a property access.";
    pub const IDENTIFIER_OR_STRING_LITERAL_EXPECTED: &str =
        "Identifier or string literal expected.";
    pub const THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H:
        &str = "The current file is a CommonJS module whose imports will produce 'require' calls; however, the referenced file is an ECMAScript module and cannot be imported with 'require'. Consider writing a dynamic 'import(\"{0}\")' call instead.";
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_CHANGE_ITS_FILE_EXTENSION_TO_OR_CRE:
        &str = "To convert this file to an ECMAScript module, change its file extension to '{0}' or create a local package.json file with `{ \"type\": \"module\" }`.";
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_CHANGE_ITS_FILE_EXTENSION_TO_OR_ADD:
        &str = "To convert this file to an ECMAScript module, change its file extension to '{0}', or add the field `\"type\": \"module\"` to '{1}'.";
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_ADD_THE_FIELD_TYPE_MODULE_TO: &str = "To convert this file to an ECMAScript module, add the field `\"type\": \"module\"` to '{0}'.";
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_CREATE_A_LOCAL_PACKAGE_JSON_FILE_WI:
        &str = "To convert this file to an ECMAScript module, create a local package.json file with `{ \"type\": \"module\" }`.";
    pub const IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA:
        &str = "'{0}' is a type and must be imported using a type-only import when 'verbatimModuleSyntax' is enabled.";
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPOR:
        &str = "'{0}' resolves to a type-only declaration and must be imported using a type-only import when 'verbatimModuleSyntax' is enabled.";
    pub const DECORATOR_USED_BEFORE_EXPORT_HERE: &str = "Decorator used before 'export' here.";
    pub const OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX: &str =
        "Octal escape sequences are not allowed. Use the syntax '{0}'.";
    pub const ESCAPE_SEQUENCE_IS_NOT_ALLOWED: &str = "Escape sequence '{0}' is not allowed.";
    pub const DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED: &str =
        "Decimals with leading zeros are not allowed.";
    pub const FILE_APPEARS_TO_BE_BINARY: &str = "File appears to be binary.";
    pub const MODIFIER_CANNOT_APPEAR_ON_A_USING_DECLARATION: &str =
        "'{0}' modifier cannot appear on a 'using' declaration.";
    pub const DECLARATIONS_MAY_NOT_HAVE_BINDING_PATTERNS: &str =
        "'{0}' declarations may not have binding patterns.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_USING_DECLARATION: &str =
        "The left-hand side of a 'for...in' statement cannot be a 'using' declaration.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_AN_AWAIT_USING_DECLARATION: &str =
        "The left-hand side of a 'for...in' statement cannot be an 'await using' declaration.";
    pub const MODIFIER_CANNOT_APPEAR_ON_AN_AWAIT_USING_DECLARATION: &str =
        "'{0}' modifier cannot appear on an 'await using' declaration.";
    pub const IDENTIFIER_STRING_LITERAL_OR_NUMBER_LITERAL_EXPECTED: &str =
        "Identifier, string literal, or number literal expected.";
    pub const EXPRESSION_MUST_BE_ENCLOSED_IN_PARENTHESES_TO_BE_USED_AS_A_DECORATOR: &str =
        "Expression must be enclosed in parentheses to be used as a decorator.";
    pub const INVALID_SYNTAX_IN_DECORATOR: &str = "Invalid syntax in decorator.";
    pub const UNKNOWN_REGULAR_EXPRESSION_FLAG: &str = "Unknown regular expression flag.";
    pub const DUPLICATE_REGULAR_EXPRESSION_FLAG: &str = "Duplicate regular expression flag.";
    pub const THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER: &str =
        "This regular expression flag is only available when targeting '{0}' or later.";
    pub const THE_UNICODE_U_FLAG_AND_THE_UNICODE_SETS_V_FLAG_CANNOT_BE_SET_SIMULTANEOUSLY: &str =
        "The Unicode (u) flag and the Unicode Sets (v) flag cannot be set simultaneously.";
    pub const NAMED_CAPTURING_GROUPS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ES2018_OR_LATER: &str =
        "Named capturing groups are only available when targeting 'ES2018' or later.";
    pub const SUBPATTERN_FLAGS_MUST_BE_PRESENT_WHEN_THERE_IS_A_MINUS_SIGN: &str =
        "Subpattern flags must be present when there is a minus sign.";
    pub const INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED: &str = "Incomplete quantifier. Digit expected.";
    pub const NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER: &str = "Numbers out of order in quantifier.";
    pub const THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION: &str =
        "There is nothing available for repetition.";
    pub const UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH: &str =
        "Unexpected '{0}'. Did you mean to escape it with backslash?";
    pub const THIS_REGULAR_EXPRESSION_FLAG_CANNOT_BE_TOGGLED_WITHIN_A_SUBPATTERN: &str =
        "This regular expression flag cannot be toggled within a subpattern.";
    pub const K_MUST_BE_FOLLOWED_BY_A_CAPTURING_GROUP_NAME_ENCLOSED_IN_ANGLE_BRACKETS: &str =
        "'\\k' must be followed by a capturing group name enclosed in angle brackets.";
    pub const Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS: &str =
        "'\\q' is only available inside character class.";
    pub const C_MUST_BE_FOLLOWED_BY_AN_ASCII_LETTER: &str =
        "'\\c' must be followed by an ASCII letter.";
    pub const UNDETERMINED_CHARACTER_ESCAPE: &str = "Undetermined character escape.";
    pub const EXPECTED_A_CAPTURING_GROUP_NAME: &str = "Expected a capturing group name.";
    pub const NAMED_CAPTURING_GROUPS_WITH_THE_SAME_NAME_MUST_BE_MUTUALLY_EXCLUSIVE_TO_EACH_OTH:
        &str =
        "Named capturing groups with the same name must be mutually exclusive to each other.";
    pub const A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS: &str =
        "A character class range must not be bounded by another character class.";
    pub const RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS: &str =
        "Range out of order in character class.";
    pub const ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID:
        &str = "Anything that would possibly match more than a single character is invalid inside a negated character class.";
    pub const OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_I:
        &str =
        "Operators must not be mixed within a character class. Wrap it in a nested class instead.";
    pub const EXPECTED_A_CLASS_SET_OPERAND: &str = "Expected a class set operand.";
    pub const Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES: &str =
        "'\\q' must be followed by string alternatives enclosed in braces.";
    pub const A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO:
        &str = "A character class must not contain a reserved double punctuator. Did you mean to escape it with backslash?";
    pub const EXPECTED_A_UNICODE_PROPERTY_NAME: &str = "Expected a Unicode property name.";
    pub const UNKNOWN_UNICODE_PROPERTY_NAME: &str = "Unknown Unicode property name.";
    pub const EXPECTED_A_UNICODE_PROPERTY_VALUE: &str = "Expected a Unicode property value.";
    pub const UNKNOWN_UNICODE_PROPERTY_VALUE: &str = "Unknown Unicode property value.";
    pub const EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE: &str =
        "Expected a Unicode property name or value.";
    pub const ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O:
        &str = "Any Unicode property that would possibly match more than a single character is only available when the Unicode Sets (v) flag is set.";
    pub const UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE: &str =
        "Unknown Unicode property name or value.";
    pub const UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR:
        &str = "Unicode property value expressions are only available when the Unicode (u) flag or the Unicode Sets (v) flag is set.";
    pub const MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES: &str =
        "'\\{0}' must be followed by a Unicode property value expression enclosed in braces.";
    pub const THERE_IS_NO_CAPTURING_GROUP_NAMED_IN_THIS_REGULAR_EXPRESSION: &str =
        "There is no capturing group named '{0}' in this regular expression.";
    pub const THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_ONLY_CAPTURIN:
        &str = "This backreference refers to a group that does not exist. There are only {0} capturing groups in this regular expression.";
    pub const THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_NO_CAPTURING:
        &str = "This backreference refers to a group that does not exist. There are no capturing groups in this regular expression.";
    pub const THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION: &str =
        "This character cannot be escaped in a regular expression.";
    pub const OCTAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS_I:
        &str = "Octal escape sequences and backreferences are not allowed in a character class. If this was intended as an escape sequence, use the syntax '{0}' instead.";
    pub const DECIMAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS:
        &str = "Decimal escape sequences and backreferences are not allowed in a character class.";
    pub const UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO:
        &str = "Unicode escape sequences are only available when the Unicode (u) flag or the Unicode Sets (v) flag is set.";
    pub const A_BIGINT_LITERAL_CANNOT_BE_USED_AS_A_PROPERTY_NAME: &str =
        "A 'bigint' literal cannot be used as a property name.";
    pub const A_NAMESPACE_DECLARATION_SHOULD_NOT_BE_DECLARED_USING_THE_MODULE_KEYWORD_PLEASE_U:
        &str = "A 'namespace' declaration should not be declared using the 'module' keyword. Please use the 'namespace' keyword instead.";
    pub const TYPE_ONLY_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESO:
        &str = "Type-only import of an ECMAScript module from a CommonJS module must have a 'resolution-mode' attribute.";
    pub const TYPE_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESOLUTIO:
        &str = "Type import of an ECMAScript module from a CommonJS module must have a 'resolution-mode' attribute.";
    pub const IMPORTING_A_JSON_FILE_INTO_AN_ECMASCRIPT_MODULE_REQUIRES_A_TYPE_JSON_IMPORT_ATTR:
        &str = "Importing a JSON file into an ECMAScript module requires a 'type: \"json\"' import attribute when 'module' is set to '{0}'.";
    pub const NAMED_IMPORTS_FROM_A_JSON_FILE_INTO_AN_ECMASCRIPT_MODULE_ARE_NOT_ALLOWED_WHEN_MO:
        &str = "Named imports from a JSON file into an ECMAScript module are not allowed when 'module' is set to '{0}'.";
    pub const USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: &str =
        "'using' declarations are not allowed in ambient contexts.";
    pub const AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: &str =
        "'await using' declarations are not allowed in ambient contexts.";
    pub const USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_W:
        &str = "'using' declarations are not allowed in 'case' or 'default' clauses unless contained within a block.";
    pub const AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTA:
        &str = "'await using' declarations are not allowed in 'case' or 'default' clauses unless contained within a block.";
    pub const IGNORE_THE_TSCONFIG_FOUND_AND_BUILD_WITH_COMMANDLINE_OPTIONS_AND_FILES: &str =
        "Ignore the tsconfig found and build with commandline options and files.";
    pub const THE_TYPES_OF_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES: &str =
        "The types of '{0}' are incompatible between these types.";
    pub const THE_TYPES_RETURNED_BY_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES: &str =
        "The types returned by '{0}' are incompatible between these types.";
    pub const CALL_SIGNATURE_RETURN_TYPES_AND_ARE_INCOMPATIBLE: &str =
        "Call signature return types '{0}' and '{1}' are incompatible.";
    pub const CONSTRUCT_SIGNATURE_RETURN_TYPES_AND_ARE_INCOMPATIBLE: &str =
        "Construct signature return types '{0}' and '{1}' are incompatible.";
    pub const CALL_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_AND: &str =
        "Call signatures with no arguments have incompatible return types '{0}' and '{1}'.";
    pub const CONSTRUCT_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_AND: &str =
        "Construct signatures with no arguments have incompatible return types '{0}' and '{1}'.";
    pub const THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_IMPORT_WHEN_IMPORT_TYPE_IS_USED_ON_I:
        &str = "The 'type' modifier cannot be used on a named import when 'import type' is used on its import statement.";
    pub const THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_EXPORT_WHEN_EXPORT_TYPE_IS_USED_ON_I:
        &str = "The 'type' modifier cannot be used on a named export when 'export type' is used on its export statement.";
    pub const THIS_TYPE_PARAMETER_MIGHT_NEED_AN_EXTENDS_CONSTRAINT: &str =
        "This type parameter might need an `extends {0}` constraint.";
    pub const THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_EXPORT_MAP_ENTRY_IN_FIL:
        &str = "The project root is ambiguous, but is required to resolve export map entry '{0}' in file '{1}'. Supply the `rootDir` compiler option to disambiguate.";
    pub const THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_IMPORT_MAP_ENTRY_IN_FIL:
        &str = "The project root is ambiguous, but is required to resolve import map entry '{0}' in file '{1}'. Supply the `rootDir` compiler option to disambiguate.";
    pub const ADD_EXTENDS_CONSTRAINT: &str = "Add `extends` constraint.";
    pub const ADD_EXTENDS_CONSTRAINT_TO_ALL_TYPE_PARAMETERS: &str =
        "Add `extends` constraint to all type parameters";
    pub const DUPLICATE_IDENTIFIER: &str = "Duplicate identifier '{0}'.";
    pub const INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN:
        &str = "Initializer of instance member variable '{0}' cannot reference identifier '{1}' declared in the constructor.";
    pub const STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS: &str =
        "Static members cannot reference class type parameters.";
    pub const CIRCULAR_DEFINITION_OF_IMPORT_ALIAS: &str =
        "Circular definition of import alias '{0}'.";
    pub const CANNOT_FIND_NAME: &str = "Cannot find name '{0}'.";
    pub const MODULE_HAS_NO_EXPORTED_MEMBER: &str = "Module '{0}' has no exported member '{1}'.";
    pub const FILE_IS_NOT_A_MODULE: &str = "File '{0}' is not a module.";
    pub const CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS: &str =
        "Cannot find module '{0}' or its corresponding type declarations.";
    pub const MODULE_HAS_ALREADY_EXPORTED_A_MEMBER_NAMED_CONSIDER_EXPLICITLY_RE_EXPORTING_TO_R:
        &str = "Module {0} has already exported a member named '{1}'. Consider explicitly re-exporting to resolve the ambiguity.";
    pub const AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_MODULE_WITH_OTHER_EXPORTED_ELEMENTS: &str =
        "An export assignment cannot be used in a module with other exported elements.";
    pub const TYPE_RECURSIVELY_REFERENCES_ITSELF_AS_A_BASE_TYPE: &str =
        "Type '{0}' recursively references itself as a base type.";
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION: &str =
        "Cannot find name '{0}'. Did you mean to write this in an async function?";
    pub const AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH:
        &str = "An interface can only extend an object type or intersection of object types with statically known members.";
    pub const TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT: &str =
        "Type parameter '{0}' has a circular constraint.";
    pub const GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S: &str =
        "Generic type '{0}' requires {1} type argument(s).";
    pub const TYPE_IS_NOT_GENERIC: &str = "Type '{0}' is not generic.";
    pub const GLOBAL_TYPE_MUST_BE_A_CLASS_OR_INTERFACE_TYPE: &str =
        "Global type '{0}' must be a class or interface type.";
    pub const GLOBAL_TYPE_MUST_HAVE_TYPE_PARAMETER_S: &str =
        "Global type '{0}' must have {1} type parameter(s).";
    pub const CANNOT_FIND_GLOBAL_TYPE: &str = "Cannot find global type '{0}'.";
    pub const NAMED_PROPERTY_OF_TYPES_AND_ARE_NOT_IDENTICAL: &str =
        "Named property '{0}' of types '{1}' and '{2}' are not identical.";
    pub const INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND: &str =
        "Interface '{0}' cannot simultaneously extend types '{1}' and '{2}'.";
    pub const EXCESSIVE_STACK_DEPTH_COMPARING_TYPES_AND: &str =
        "Excessive stack depth comparing types '{0}' and '{1}'.";
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE: &str = "Type '{0}' is not assignable to type '{1}'.";
    pub const CANNOT_REDECLARE_EXPORTED_VARIABLE: &str =
        "Cannot redeclare exported variable '{0}'.";
    pub const PROPERTY_IS_MISSING_IN_TYPE: &str = "Property '{0}' is missing in type '{1}'.";
    pub const PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE: &str =
        "Property '{0}' is private in type '{1}' but not in type '{2}'.";
    pub const TYPES_OF_PROPERTY_ARE_INCOMPATIBLE: &str =
        "Types of property '{0}' are incompatible.";
    pub const PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE: &str =
        "Property '{0}' is optional in type '{1}' but required in type '{2}'.";
    pub const TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE: &str =
        "Types of parameters '{0}' and '{1}' are incompatible.";
    pub const INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE: &str =
        "Index signature for type '{0}' is missing in type '{1}'.";
    pub const AND_INDEX_SIGNATURES_ARE_INCOMPATIBLE: &str =
        "'{0}' and '{1}' index signatures are incompatible.";
    pub const THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY: &str =
        "'this' cannot be referenced in a module or namespace body.";
    pub const THIS_CANNOT_BE_REFERENCED_IN_CURRENT_LOCATION: &str =
        "'this' cannot be referenced in current location.";
    pub const THIS_CANNOT_BE_REFERENCED_IN_A_STATIC_PROPERTY_INITIALIZER: &str =
        "'this' cannot be referenced in a static property initializer.";
    pub const SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS: &str =
        "'super' can only be referenced in a derived class.";
    pub const SUPER_CANNOT_BE_REFERENCED_IN_CONSTRUCTOR_ARGUMENTS: &str =
        "'super' cannot be referenced in constructor arguments.";
    pub const SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE:
        &str = "Super calls are not permitted outside constructors or in nested functions inside constructors.";
    pub const SUPER_PROPERTY_ACCESS_IS_PERMITTED_ONLY_IN_A_CONSTRUCTOR_MEMBER_FUNCTION_OR_MEMB:
        &str = "'super' property access is permitted only in a constructor, member function, or member accessor of a derived class.";
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE: &str =
        "Property '{0}' does not exist on type '{1}'.";
    pub const ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER:
        &str = "Only public and protected methods of the base class are accessible via the 'super' keyword.";
    pub const PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS: &str =
        "Property '{0}' is private and only accessible within class '{1}'.";
    pub const THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WHICH_DOES_NOT_EXIST_IN_CONSIDER_U:
        &str = "This syntax requires an imported helper named '{1}' which does not exist in '{0}'. Consider upgrading your version of '{0}'.";
    pub const TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT: &str =
        "Type '{0}' does not satisfy the constraint '{1}'.";
    pub const ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE: &str =
        "Argument of type '{0}' is not assignable to parameter of type '{1}'.";
    pub const CALL_TARGET_DOES_NOT_CONTAIN_ANY_SIGNATURES: &str =
        "Call target does not contain any signatures.";
    pub const UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS: &str =
        "Untyped function calls may not accept type arguments.";
    pub const VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW: &str =
        "Value of type '{0}' is not callable. Did you mean to include 'new'?";
    pub const THIS_EXPRESSION_IS_NOT_CALLABLE: &str = "This expression is not callable.";
    pub const ONLY_A_VOID_FUNCTION_CAN_BE_CALLED_WITH_THE_NEW_KEYWORD: &str =
        "Only a void function can be called with the 'new' keyword.";
    pub const THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE: &str = "This expression is not constructable.";
    pub const CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV:
        &str = "Conversion of type '{0}' to type '{1}' may be a mistake because neither type sufficiently overlaps with the other. If this was intentional, convert the expression to 'unknown' first.";
    pub const OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE: &str =
        "Object literal may only specify known properties, and '{0}' does not exist in type '{1}'.";
    pub const THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_BUT_MODULE_CANNOT_BE_FOUND: &str =
        "This syntax requires an imported helper but module '{0}' cannot be found.";
    pub const A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V:
        &str = "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.";
    pub const AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE: &str =
        "An arithmetic operand must be of type 'any', 'number', 'bigint' or an enum type.";
    pub const THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPER:
        &str = "The operand of an increment or decrement operator must be a variable or a property access.";
    pub const THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP:
        &str = "The left-hand side of an 'instanceof' expression must be of type 'any', an object type or a type parameter.";
    pub const THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLA:
        &str = "The right-hand side of an 'instanceof' expression must be either of type 'any', a class, function, or other type assignable to the 'Function' interface type, or an object type with a 'Symbol.hasInstance' method.";
    pub const THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT:
        &str = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";
    pub const THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT:
        &str = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";
    pub const THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY:
        &str =
        "The left-hand side of an assignment expression must be a variable or a property access.";
    pub const OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND: &str =
        "Operator '{0}' cannot be applied to types '{1}' and '{2}'.";
    pub const FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE:
        &str =
        "Function lacks ending return statement and return type does not include 'undefined'.";
    pub const THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA:
        &str = "This comparison appears to be unintentional because the types '{0}' and '{1}' have no overlap.";
    pub const TYPE_PARAMETER_NAME_CANNOT_BE: &str = "Type parameter name cannot be '{0}'.";
    pub const A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION: &str =
        "A parameter property is only allowed in a constructor implementation.";
    pub const A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE: &str =
        "A rest parameter must be of an array type.";
    pub const A_PARAMETER_INITIALIZER_IS_ONLY_ALLOWED_IN_A_FUNCTION_OR_CONSTRUCTOR_IMPLEMENTAT:
        &str =
        "A parameter initializer is only allowed in a function or constructor implementation.";
    pub const PARAMETER_CANNOT_REFERENCE_ITSELF: &str = "Parameter '{0}' cannot reference itself.";
    pub const PARAMETER_CANNOT_REFERENCE_IDENTIFIER_DECLARED_AFTER_IT: &str =
        "Parameter '{0}' cannot reference identifier '{1}' declared after it.";
    pub const DUPLICATE_INDEX_SIGNATURE_FOR_TYPE: &str =
        "Duplicate index signature for type '{0}'.";
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD:
        &str = "Type '{0}' is not assignable to type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties.";
    pub const A_SUPER_CALL_MUST_BE_THE_FIRST_STATEMENT_IN_THE_CONSTRUCTOR_TO_REFER_TO_SUPER_OR:
        &str = "A 'super' call must be the first statement in the constructor to refer to 'super' or 'this' when a derived class contains initialized properties, parameter properties, or private identifiers.";
    pub const CONSTRUCTORS_FOR_DERIVED_CLASSES_MUST_CONTAIN_A_SUPER_CALL: &str =
        "Constructors for derived classes must contain a 'super' call.";
    pub const A_GET_ACCESSOR_MUST_RETURN_A_VALUE: &str = "A 'get' accessor must return a value.";
    pub const ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_WITH_EXACTOPTIONALPROPER:
        &str = "Argument of type '{0}' is not assignable to parameter of type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties.";
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_EXPORTED_OR_NON_EXPORTED: &str =
        "Overload signatures must all be exported or non-exported.";
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT: &str =
        "Overload signatures must all be ambient or non-ambient.";
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED: &str =
        "Overload signatures must all be public, private or protected.";
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED: &str =
        "Overload signatures must all be optional or required.";
    pub const FUNCTION_OVERLOAD_MUST_BE_STATIC: &str = "Function overload must be static.";
    pub const FUNCTION_OVERLOAD_MUST_NOT_BE_STATIC: &str = "Function overload must not be static.";
    pub const FUNCTION_IMPLEMENTATION_NAME_MUST_BE: &str =
        "Function implementation name must be '{0}'.";
    pub const CONSTRUCTOR_IMPLEMENTATION_IS_MISSING: &str =
        "Constructor implementation is missing.";
    pub const FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION:
        &str = "Function implementation is missing or not immediately following the declaration.";
    pub const MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED: &str =
        "Multiple constructor implementations are not allowed.";
    pub const DUPLICATE_FUNCTION_IMPLEMENTATION: &str = "Duplicate function implementation.";
    pub const THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE: &str =
        "This overload signature is not compatible with its implementation signature.";
    pub const INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL:
        &str =
        "Individual declarations in merged declaration '{0}' must be all exported or all local.";
    pub const DUPLICATE_IDENTIFIER_ARGUMENTS_COMPILER_USES_ARGUMENTS_TO_INITIALIZE_REST_PARAME:
        &str = "Duplicate identifier 'arguments'. Compiler uses 'arguments' to initialize rest parameters.";
    pub const DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER: &str =
        "Declaration name conflicts with built-in global identifier '{0}'.";
    pub const CONSTRUCTOR_CANNOT_BE_USED_AS_A_PARAMETER_PROPERTY_NAME: &str =
        "'constructor' cannot be used as a parameter property name.";
    pub const DUPLICATE_IDENTIFIER_THIS_COMPILER_USES_VARIABLE_DECLARATION_THIS_TO_CAPTURE_THI:
        &str = "Duplicate identifier '_this'. Compiler uses variable declaration '_this' to capture 'this' reference.";
    pub const EXPRESSION_RESOLVES_TO_VARIABLE_DECLARATION_THIS_THAT_COMPILER_USES_TO_CAPTURE_T:
        &str = "Expression resolves to variable declaration '_this' that compiler uses to capture 'this' reference.";
    pub const A_SUPER_CALL_MUST_BE_A_ROOT_LEVEL_STATEMENT_WITHIN_A_CONSTRUCTOR_OF_A_DERIVED_CL:
        &str = "A 'super' call must be a root-level statement within a constructor of a derived class that contains initialized properties, parameter properties, or private identifiers.";
    pub const EXPRESSION_RESOLVES_TO_SUPER_THAT_COMPILER_USES_TO_CAPTURE_BASE_CLASS_REFERENCE:
        &str =
        "Expression resolves to '_super' that compiler uses to capture base class reference.";
    pub const SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP:
        &str = "Subsequent variable declarations must have the same type.  Variable '{0}' must be of type '{1}', but here has type '{2}'.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION: &str =
        "The left-hand side of a 'for...in' statement cannot use a type annotation.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY: &str =
        "The left-hand side of a 'for...in' statement must be of type 'string' or 'any'.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS:
        &str =
        "The left-hand side of a 'for...in' statement must be a variable or a property access.";
    pub const THE_RIGHT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR:
        &str = "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type '{0}'.";
    pub const SETTERS_CANNOT_RETURN_A_VALUE: &str = "Setters cannot return a value.";
    pub const RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF:
        &str = "Return type of constructor signature must be assignable to the instance type of the class.";
    pub const THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A:
        &str = "The 'with' statement is not supported. All symbols in a 'with' block will have type 'any'.";
    pub const PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE: &str =
        "Property '{0}' of type '{1}' is not assignable to '{2}' index type '{3}'.";
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2:
        &str = "Type '{0}' is not assignable to type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the type of the target.";
    pub const INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE: &str =
        "'{0}' index type '{1}' is not assignable to '{2}' index type '{3}'.";
    pub const CLASS_NAME_CANNOT_BE: &str = "Class name cannot be '{0}'.";
    pub const CLASS_INCORRECTLY_EXTENDS_BASE_CLASS: &str =
        "Class '{0}' incorrectly extends base class '{1}'.";
    pub const PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE: &str =
        "Property '{0}' in type '{1}' is not assignable to the same property in base type '{2}'.";
    pub const CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE: &str =
        "Class static side '{0}' incorrectly extends base class static side '{1}'.";
    pub const TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE: &str =
        "Type of computed property's value is '{0}', which is not assignable to type '{1}'.";
    pub const TYPES_OF_CONSTRUCT_SIGNATURES_ARE_INCOMPATIBLE: &str =
        "Types of construct signatures are incompatible.";
    pub const CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE: &str =
        "Class '{0}' incorrectly implements interface '{1}'.";
    pub const A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S:
        &str = "A class can only implement an object type or intersection of object types with statically known members.";
    pub const CLASS_DEFINES_INSTANCE_MEMBER_FUNCTION_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE:
        &str = "Class '{0}' defines instance member function '{1}', but extended class '{2}' defines it as instance member accessor.";
    pub const CLASS_DEFINES_INSTANCE_MEMBER_PROPERTY_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE:
        &str = "Class '{0}' defines instance member property '{1}', but extended class '{2}' defines it as instance member function.";
    pub const CLASS_DEFINES_INSTANCE_MEMBER_ACCESSOR_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE:
        &str = "Class '{0}' defines instance member accessor '{1}', but extended class '{2}' defines it as instance member function.";
    pub const INTERFACE_NAME_CANNOT_BE: &str = "Interface name cannot be '{0}'.";
    pub const ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS: &str =
        "All declarations of '{0}' must have identical type parameters.";
    pub const INTERFACE_INCORRECTLY_EXTENDS_INTERFACE: &str =
        "Interface '{0}' incorrectly extends interface '{1}'.";
    pub const ENUM_NAME_CANNOT_BE: &str = "Enum name cannot be '{0}'.";
    pub const IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ:
        &str = "In an enum with multiple declarations, only one declaration can omit an initializer for its first enum element.";
    pub const A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W:
        &str = "A namespace declaration cannot be in a different file from a class or function with which it is merged.";
    pub const A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC:
        &str = "A namespace declaration cannot be located prior to a class or function with which it is merged.";
    pub const AMBIENT_MODULES_CANNOT_BE_NESTED_IN_OTHER_MODULES_OR_NAMESPACES: &str =
        "Ambient modules cannot be nested in other modules or namespaces.";
    pub const AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME: &str =
        "Ambient module declaration cannot specify relative module name.";
    pub const MODULE_IS_HIDDEN_BY_A_LOCAL_DECLARATION_WITH_THE_SAME_NAME: &str =
        "Module '{0}' is hidden by a local declaration with the same name.";
    pub const IMPORT_NAME_CANNOT_BE: &str = "Import name cannot be '{0}'.";
    pub const IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M:
        &str = "Import or export declaration in an ambient module declaration cannot reference module through relative module name.";
    pub const IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF: &str =
        "Import declaration conflicts with local declaration of '{0}'.";
    pub const DUPLICATE_IDENTIFIER_COMPILER_RESERVES_NAME_IN_TOP_LEVEL_SCOPE_OF_A_MODULE: &str =
        "Duplicate identifier '{0}'. Compiler reserves name '{1}' in top level scope of a module.";
    pub const TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY: &str =
        "Types have separate declarations of a private property '{0}'.";
    pub const PROPERTY_IS_PROTECTED_BUT_TYPE_IS_NOT_A_CLASS_DERIVED_FROM: &str =
        "Property '{0}' is protected but type '{1}' is not a class derived from '{2}'.";
    pub const PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE: &str =
        "Property '{0}' is protected in type '{1}' but public in type '{2}'.";
    pub const PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES: &str =
        "Property '{0}' is protected and only accessible within class '{1}' and its subclasses.";
    pub const PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A:
        &str = "Property '{0}' is protected and only accessible through an instance of class '{1}'. This is an instance of class '{2}'.";
    pub const THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD: &str =
        "The '{0}' operator is not allowed for boolean types. Consider using '{1}' instead.";
    pub const BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION: &str =
        "Block-scoped variable '{0}' used before its declaration.";
    pub const CLASS_USED_BEFORE_ITS_DECLARATION: &str = "Class '{0}' used before its declaration.";
    pub const ENUM_USED_BEFORE_ITS_DECLARATION: &str = "Enum '{0}' used before its declaration.";
    pub const CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE: &str =
        "Cannot redeclare block-scoped variable '{0}'.";
    pub const AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME: &str =
        "An enum member cannot have a numeric name.";
    pub const VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED: &str =
        "Variable '{0}' is used before being assigned.";
    pub const TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF: &str =
        "Type alias '{0}' circularly references itself.";
    pub const TYPE_ALIAS_NAME_CANNOT_BE: &str = "Type alias name cannot be '{0}'.";
    pub const AN_AMD_MODULE_CANNOT_HAVE_MULTIPLE_NAME_ASSIGNMENTS: &str =
        "An AMD module cannot have multiple name assignments.";
    pub const MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED: &str =
        "Module '{0}' declares '{1}' locally, but it is not exported.";
    pub const MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS: &str =
        "Module '{0}' declares '{1}' locally, but it is exported as '{2}'.";
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE: &str = "Type '{0}' is not an array type.";
    pub const A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN: &str =
        "A rest element must be last in a destructuring pattern.";
    pub const A_BINDING_PATTERN_PARAMETER_CANNOT_BE_OPTIONAL_IN_AN_IMPLEMENTATION_SIGNATURE: &str =
        "A binding pattern parameter cannot be optional in an implementation signature.";
    pub const A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY: &str =
        "A computed property name must be of type 'string', 'number', 'symbol', or 'any'.";
    pub const THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME: &str =
        "'this' cannot be referenced in a computed property name.";
    pub const SUPER_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME: &str =
        "'super' cannot be referenced in a computed property name.";
    pub const A_COMPUTED_PROPERTY_NAME_CANNOT_REFERENCE_A_TYPE_PARAMETER_FROM_ITS_CONTAINING_T:
        &str =
        "A computed property name cannot reference a type parameter from its containing type.";
    pub const CANNOT_FIND_GLOBAL_VALUE: &str = "Cannot find global value '{0}'.";
    pub const THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL: &str =
        "The '{0}' operator cannot be applied to type 'symbol'.";
    pub const SPREAD_OPERATOR_IN_NEW_EXPRESSIONS_IS_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_5:
        &str = "Spread operator in 'new' expressions is only available when targeting ECMAScript 5 and higher.";
    pub const ENUM_DECLARATIONS_MUST_ALL_BE_CONST_OR_NON_CONST: &str =
        "Enum declarations must all be const or non-const.";
    pub const CONST_ENUM_MEMBER_INITIALIZERS_MUST_BE_CONSTANT_EXPRESSIONS: &str =
        "const enum member initializers must be constant expressions.";
    pub const CONST_ENUMS_CAN_ONLY_BE_USED_IN_PROPERTY_OR_INDEX_ACCESS_EXPRESSIONS_OR_THE_RIGH:
        &str = "'const' enums can only be used in property or index access expressions or the right hand side of an import declaration or export assignment or type query.";
    pub const A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL: &str =
        "A const enum member can only be accessed using a string literal.";
    pub const CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_A_NON_FINITE_VALUE: &str =
        "'const' enum member initializer was evaluated to a non-finite value.";
    pub const CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_DISALLOWED_VALUE_NAN: &str =
        "'const' enum member initializer was evaluated to disallowed value 'NaN'.";
    pub const LET_IS_NOT_ALLOWED_TO_BE_USED_AS_A_NAME_IN_LET_OR_CONST_DECLARATIONS: &str =
        "'let' is not allowed to be used as a name in 'let' or 'const' declarations.";
    pub const CANNOT_INITIALIZE_OUTER_SCOPED_VARIABLE_IN_THE_SAME_SCOPE_AS_BLOCK_SCOPED_DECLAR:
        &str = "Cannot initialize outer scoped variable '{0}' in the same scope as block scoped declaration '{1}'.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION: &str =
        "The left-hand side of a 'for...of' statement cannot use a type annotation.";
    pub const EXPORT_DECLARATION_CONFLICTS_WITH_EXPORTED_DECLARATION_OF: &str =
        "Export declaration conflicts with exported declaration of '{0}'.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS:
        &str =
        "The left-hand side of a 'for...of' statement must be a variable or a property access.";
    pub const TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR: &str =
        "Type '{0}' must have a '[Symbol.iterator]()' method that returns an iterator.";
    pub const AN_ITERATOR_MUST_HAVE_A_NEXT_METHOD: &str =
        "An iterator must have a 'next()' method.";
    pub const THE_TYPE_RETURNED_BY_THE_METHOD_OF_AN_ITERATOR_MUST_HAVE_A_VALUE_PROPERTY: &str =
        "The type returned by the '{0}()' method of an iterator must have a 'value' property.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN: &str =
        "The left-hand side of a 'for...in' statement cannot be a destructuring pattern.";
    pub const CANNOT_REDECLARE_IDENTIFIER_IN_CATCH_CLAUSE: &str =
        "Cannot redeclare identifier '{0}' in catch clause.";
    pub const TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX: &str =
        "Tuple type '{0}' of length '{1}' has no element at index '{2}'.";
    pub const USING_A_STRING_IN_A_FOR_OF_STATEMENT_IS_ONLY_SUPPORTED_IN_ECMASCRIPT_5_AND_HIGHE:
        &str =
        "Using a string in a 'for...of' statement is only supported in ECMAScript 5 and higher.";
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE: &str =
        "Type '{0}' is not an array type or a string type.";
    pub const THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U:
        &str = "The 'arguments' object cannot be referenced in an arrow function in ES5. Consider using a standard function expression.";
    pub const THIS_MODULE_CAN_ONLY_BE_REFERENCED_WITH_ECMASCRIPT_IMPORTS_EXPORTS_BY_TURNING_ON:
        &str = "This module can only be referenced with ECMAScript imports/exports by turning on the '{0}' flag and referencing its default export.";
    pub const MODULE_USES_EXPORT_AND_CANNOT_BE_USED_WITH_EXPORT: &str =
        "Module '{0}' uses 'export =' and cannot be used with 'export *'.";
    pub const AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG:
        &str =
        "An interface can only extend an identifier/qualified-name with optional type arguments.";
    pub const A_CLASS_CAN_ONLY_IMPLEMENT_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARGUM:
        &str =
        "A class can only implement an identifier/qualified-name with optional type arguments.";
    pub const A_REST_ELEMENT_CANNOT_CONTAIN_A_BINDING_PATTERN: &str =
        "A rest element cannot contain a binding pattern.";
    pub const IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_TYPE_ANNOTATION: &str =
        "'{0}' is referenced directly or indirectly in its own type annotation.";
    pub const CANNOT_FIND_NAMESPACE: &str = "Cannot find namespace '{0}'.";
    pub const TYPE_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR: &str =
        "Type '{0}' must have a '[Symbol.asyncIterator]()' method that returns an async iterator.";
    pub const A_GENERATOR_CANNOT_HAVE_A_VOID_TYPE_ANNOTATION: &str =
        "A generator cannot have a 'void' type annotation.";
    pub const IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION: &str =
        "'{0}' is referenced directly or indirectly in its own base expression.";
    pub const TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE: &str =
        "Type '{0}' is not a constructor function type.";
    pub const NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS: &str =
        "No base constructor has the specified number of type arguments.";
    pub const BASE_CONSTRUCTOR_RETURN_TYPE_IS_NOT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYP:
        &str = "Base constructor return type '{0}' is not an object type or intersection of object types with statically known members.";
    pub const BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE: &str =
        "Base constructors must all have the same return type.";
    pub const CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS: &str =
        "Cannot create an instance of an abstract class.";
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_ABSTRACT_OR_NON_ABSTRACT: &str =
        "Overload signatures must all be abstract or non-abstract.";
    pub const ABSTRACT_METHOD_IN_CLASS_CANNOT_BE_ACCESSED_VIA_SUPER_EXPRESSION: &str =
        "Abstract method '{0}' in class '{1}' cannot be accessed via super expression.";
    pub const A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE: &str =
        "A tuple type cannot be indexed with a negative value.";
    pub const NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS: &str = "Non-abstract class '{0}' does not implement inherited abstract member {1} from class '{2}'.";
    pub const ALL_DECLARATIONS_OF_AN_ABSTRACT_METHOD_MUST_BE_CONSECUTIVE: &str =
        "All declarations of an abstract method must be consecutive.";
    pub const CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE: &str =
        "Cannot assign an abstract constructor type to a non-abstract constructor type.";
    pub const A_THIS_BASED_TYPE_GUARD_IS_NOT_COMPATIBLE_WITH_A_PARAMETER_BASED_TYPE_GUARD: &str =
        "A 'this'-based type guard is not compatible with a parameter-based type guard.";
    pub const AN_ASYNC_ITERATOR_MUST_HAVE_A_NEXT_METHOD: &str =
        "An async iterator must have a 'next()' method.";
    pub const DUPLICATE_IDENTIFIER_COMPILER_USES_DECLARATION_TO_SUPPORT_ASYNC_FUNCTIONS: &str =
        "Duplicate identifier '{0}'. Compiler uses declaration '{1}' to support async functions.";
    pub const THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5:
        &str = "The 'arguments' object cannot be referenced in an async function or method in ES5. Consider using a standard function or method.";
    pub const YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER: &str =
        "'yield' expressions cannot be used in a parameter initializer.";
    pub const AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER: &str =
        "'await' expressions cannot be used in a parameter initializer.";
    pub const A_THIS_TYPE_IS_AVAILABLE_ONLY_IN_A_NON_STATIC_MEMBER_OF_A_CLASS_OR_INTERFACE: &str =
        "A 'this' type is available only in a non-static member of a class or interface.";
    pub const THE_INFERRED_TYPE_OF_REFERENCES_AN_INACCESSIBLE_TYPE_A_TYPE_ANNOTATION_IS_NECESS:
        &str = "The inferred type of '{0}' references an inaccessible '{1}' type. A type annotation is necessary.";
    pub const A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS: &str =
        "A module cannot have multiple default exports.";
    pub const DUPLICATE_IDENTIFIER_COMPILER_RESERVES_NAME_IN_TOP_LEVEL_SCOPE_OF_A_MODULE_CONTA:
        &str = "Duplicate identifier '{0}'. Compiler reserves name '{1}' in top level scope of a module containing async functions.";
    pub const PROPERTY_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE: &str =
        "Property '{0}' is incompatible with index signature.";
    pub const OBJECT_IS_POSSIBLY_NULL: &str = "Object is possibly 'null'.";
    pub const OBJECT_IS_POSSIBLY_UNDEFINED: &str = "Object is possibly 'undefined'.";
    pub const OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED: &str =
        "Object is possibly 'null' or 'undefined'.";
    pub const A_FUNCTION_RETURNING_NEVER_CANNOT_HAVE_A_REACHABLE_END_POINT: &str =
        "A function returning 'never' cannot have a reachable end point.";
    pub const TYPE_CANNOT_BE_USED_TO_INDEX_TYPE: &str =
        "Type '{0}' cannot be used to index type '{1}'.";
    pub const TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE: &str =
        "Type '{0}' has no matching index signature for type '{1}'.";
    pub const TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE: &str =
        "Type '{0}' cannot be used as an index type.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_NOT_A_VARIABLE: &str =
        "Cannot assign to '{0}' because it is not a variable.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY: &str =
        "Cannot assign to '{0}' because it is a read-only property.";
    pub const INDEX_SIGNATURE_IN_TYPE_ONLY_PERMITS_READING: &str =
        "Index signature in type '{0}' only permits reading.";
    pub const DUPLICATE_IDENTIFIER_NEWTARGET_COMPILER_USES_VARIABLE_DECLARATION_NEWTARGET_TO_C:
        &str = "Duplicate identifier '_newTarget'. Compiler uses variable declaration '_newTarget' to capture 'new.target' meta-property reference.";
    pub const EXPRESSION_RESOLVES_TO_VARIABLE_DECLARATION_NEWTARGET_THAT_COMPILER_USES_TO_CAPT:
        &str = "Expression resolves to variable declaration '_newTarget' that compiler uses to capture 'new.target' meta-property reference.";
    pub const A_MIXIN_CLASS_MUST_HAVE_A_CONSTRUCTOR_WITH_A_SINGLE_REST_PARAMETER_OF_TYPE_ANY: &str =
        "A mixin class must have a constructor with a single rest parameter of type 'any[]'.";
    pub const THE_TYPE_RETURNED_BY_THE_METHOD_OF_AN_ASYNC_ITERATOR_MUST_BE_A_PROMISE_FOR_A_TYP:
        &str = "The type returned by the '{0}()' method of an async iterator must be a promise for a type with a 'value' property.";
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE_OR_DOES_NOT_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS:
        &str = "Type '{0}' is not an array type or does not have a '[Symbol.iterator]()' method that returns an iterator.";
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE_OR_DOES_NOT_HAVE_A_SYMBOL_ITERATOR_ME:
        &str = "Type '{0}' is not an array type or a string type or does not have a '[Symbol.iterator]()' method that returns an iterator.";
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CH:
        &str = "Property '{0}' does not exist on type '{1}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{2}' or later.";
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN: &str =
        "Property '{0}' does not exist on type '{1}'. Did you mean '{2}'?";
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN: &str = "Cannot find name '{0}'. Did you mean '{1}'?";
    pub const COMPUTED_VALUES_ARE_NOT_PERMITTED_IN_AN_ENUM_WITH_STRING_VALUED_MEMBERS: &str =
        "Computed values are not permitted in an enum with string valued members.";
    pub const EXPECTED_ARGUMENTS_BUT_GOT: &str = "Expected {0} arguments, but got {1}.";
    pub const EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT: &str =
        "Expected at least {0} arguments, but got {1}.";
    pub const A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER:
        &str = "A spread argument must either have a tuple type or be passed to a rest parameter.";
    pub const EXPECTED_TYPE_ARGUMENTS_BUT_GOT: &str = "Expected {0} type arguments, but got {1}.";
    pub const TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE: &str =
        "Type '{0}' has no properties in common with type '{1}'.";
    pub const VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT: &str =
        "Value of type '{0}' has no properties in common with type '{1}'. Did you mean to call it?";
    pub const OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID:
        &str = "Object literal may only specify known properties, but '{0}' does not exist in type '{1}'. Did you mean to write '{2}'?";
    pub const BASE_CLASS_EXPRESSIONS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS: &str =
        "Base class expressions cannot reference class type parameters.";
    pub const THE_CONTAINING_FUNCTION_OR_MODULE_BODY_IS_TOO_LARGE_FOR_CONTROL_FLOW_ANALYSIS: &str =
        "The containing function or module body is too large for control flow analysis.";
    pub const PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR: &str =
        "Property '{0}' has no initializer and is not definitely assigned in the constructor.";
    pub const PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED: &str =
        "Property '{0}' is used before being assigned.";
    pub const A_REST_ELEMENT_CANNOT_HAVE_A_PROPERTY_NAME: &str =
        "A rest element cannot have a property name.";
    pub const ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS: &str =
        "Enum declarations can only merge with namespace or other enum declarations.";
    pub const PROPERTY_MAY_NOT_EXIST_ON_TYPE_DID_YOU_MEAN: &str =
        "Property '{0}' may not exist on type '{1}'. Did you mean '{2}'?";
    pub const COULD_NOT_FIND_NAME_DID_YOU_MEAN: &str =
        "Could not find name '{0}'. Did you mean '{1}'?";
    pub const OBJECT_IS_OF_TYPE_UNKNOWN: &str = "Object is of type 'unknown'.";
    pub const A_REST_ELEMENT_TYPE_MUST_BE_AN_ARRAY_TYPE: &str =
        "A rest element type must be an array type.";
    pub const NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM:
        &str = "No overload expects {0} arguments, but overloads do exist that expect either {1} or {2} arguments.";
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD:
        &str = "Property '{0}' does not exist on type '{1}'. Did you mean to access the static member '{2}' instead?";
    pub const RETURN_TYPE_ANNOTATION_CIRCULARLY_REFERENCES_ITSELF: &str =
        "Return type annotation circularly references itself.";
    pub const UNUSED_TS_EXPECT_ERROR_DIRECTIVE: &str = "Unused '@ts-expect-error' directive.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node`.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for jQuery? Try `npm i --save-dev @types/jquery`.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for a test runner? Try `npm i --save-dev @types/jest` or `npm i --save-dev @types/mocha`.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB:
        &str = "Cannot find name '{0}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{1}' or later.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2:
        &str = "Cannot find name '{0}'. Do you need to change your target library? Try changing the 'lib' compiler option to include 'dom'.";
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO:
        &str = "'{0}' only refers to a type, but is being used as a value here. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CONSTANT: &str =
        "Cannot assign to '{0}' because it is a constant.";
    pub const TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE: &str =
        "Type instantiation is excessively deep and possibly infinite.";
    pub const EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT: &str =
        "Expression produces a union type that is too complex to represent.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node` and then add 'node' to the types field in your tsconfig.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA_2:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for jQuery? Try `npm i --save-dev @types/jquery` and then add 'jquery' to the types field in your tsconfig.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N_2:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for a test runner? Try `npm i --save-dev @types/jest` or `npm i --save-dev @types/mocha` and then add 'jest' or 'mocha' to the types field in your tsconfig.";
    pub const THIS_MODULE_IS_DECLARED_WITH_EXPORT_AND_CAN_ONLY_BE_USED_WITH_A_DEFAULT_IMPORT_W:
        &str = "This module is declared with 'export =', and can only be used with a default import when using the '{0}' flag.";
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_A_DEFAULT_IMPORT: &str =
        "'{0}' can only be imported by using a default import.";
    pub const CAN_ONLY_BE_IMPORTED_BY_TURNING_ON_THE_ESMODULEINTEROP_FLAG_AND_USING_A_DEFAULT:
        &str = "'{0}' can only be imported by turning on the 'esModuleInterop' flag and using a default import.";
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_A_REQUIRE_CALL_OR_BY_USING_A_DEFAULT_IMPORT: &str =
        "'{0}' can only be imported by using a 'require' call or by using a default import.";
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_A_REQUIRE_CALL_OR_BY_TURNING_ON_THE_ESMODULEINTERO:
        &str = "'{0}' can only be imported by using a 'require' call or by turning on the 'esModuleInterop' flag and using a default import.";
    pub const JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_THE_GLOBAL_TYPE_JSX_ELEMENT_DOES_NOT:
        &str = "JSX element implicitly has type 'any' because the global type 'JSX.Element' does not exist.";
    pub const PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE: &str =
        "Property '{0}' in type '{1}' is not assignable to type '{2}'.";
    pub const JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES: &str =
        "JSX element type '{0}' does not have any construct or call signatures.";
    pub const PROPERTY_OF_JSX_SPREAD_ATTRIBUTE_IS_NOT_ASSIGNABLE_TO_TARGET_PROPERTY: &str =
        "Property '{0}' of JSX spread attribute is not assignable to target property.";
    pub const JSX_ELEMENT_CLASS_DOES_NOT_SUPPORT_ATTRIBUTES_BECAUSE_IT_DOES_NOT_HAVE_A_PROPERT:
        &str =
        "JSX element class does not support attributes because it does not have a '{0}' property.";
    pub const THE_GLOBAL_TYPE_JSX_MAY_NOT_HAVE_MORE_THAN_ONE_PROPERTY: &str =
        "The global type 'JSX.{0}' may not have more than one property.";
    pub const JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE: &str =
        "JSX spread child must be an array type.";
    pub const IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP:
        &str = "'{0}' is defined as an accessor in class '{1}', but is overridden here in '{2}' as an instance property.";
    pub const IS_DEFINED_AS_A_PROPERTY_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_ACCESSOR: &str = "'{0}' is defined as a property in class '{1}', but is overridden here in '{2}' as an accessor.";
    pub const PROPERTY_WILL_OVERWRITE_THE_BASE_PROPERTY_IN_IF_THIS_IS_INTENTIONAL_ADD_AN_INITI:
        &str = "Property '{0}' will overwrite the base property in '{1}'. If this is intentional, add an initializer. Otherwise, add a 'declare' modifier or remove the redundant declaration.";
    pub const MODULE_HAS_NO_DEFAULT_EXPORT_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD: &str = "Module '{0}' has no default export. Did you mean to use 'import { {1} } from {0}' instead?";
    pub const MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD: &str = "Module '{0}' has no exported member '{1}'. Did you mean to use 'import {1} from {0}' instead?";
    pub const TYPE_OF_PROPERTY_CIRCULARLY_REFERENCES_ITSELF_IN_MAPPED_TYPE: &str =
        "Type of property '{0}' circularly references itself in mapped type '{1}'.";
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_A_DEFAULT_IMPORT: &str =
        "'{0}' can only be imported by using 'import {1} = require({2})' or a default import.";
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_BY_TURNING_ON_THE_ESMODULEINTERO:
        &str = "'{0}' can only be imported by using 'import {1} = require({2})' or by turning on the 'esModuleInterop' flag and using a default import.";
    pub const SOURCE_HAS_ELEMENT_S_BUT_TARGET_REQUIRES: &str =
        "Source has {0} element(s) but target requires {1}.";
    pub const SOURCE_HAS_ELEMENT_S_BUT_TARGET_ALLOWS_ONLY: &str =
        "Source has {0} element(s) but target allows only {1}.";
    pub const TARGET_REQUIRES_ELEMENT_S_BUT_SOURCE_MAY_HAVE_FEWER: &str =
        "Target requires {0} element(s) but source may have fewer.";
    pub const TARGET_ALLOWS_ONLY_ELEMENT_S_BUT_SOURCE_MAY_HAVE_MORE: &str =
        "Target allows only {0} element(s) but source may have more.";
    pub const SOURCE_PROVIDES_NO_MATCH_FOR_REQUIRED_ELEMENT_AT_POSITION_IN_TARGET: &str =
        "Source provides no match for required element at position {0} in target.";
    pub const SOURCE_PROVIDES_NO_MATCH_FOR_VARIADIC_ELEMENT_AT_POSITION_IN_TARGET: &str =
        "Source provides no match for variadic element at position {0} in target.";
    pub const VARIADIC_ELEMENT_AT_POSITION_IN_SOURCE_DOES_NOT_MATCH_ELEMENT_AT_POSITION_IN_TAR:
        &str = "Variadic element at position {0} in source does not match element at position {1} in target.";
    pub const TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET: &str =
        "Type at position {0} in source is not compatible with type at position {1} in target.";
    pub const TYPE_AT_POSITIONS_THROUGH_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_T:
        &str = "Type at positions {0} through {1} in source is not compatible with type at position {2} in target.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_ENUM: &str =
        "Cannot assign to '{0}' because it is an enum.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CLASS: &str =
        "Cannot assign to '{0}' because it is a class.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_FUNCTION: &str =
        "Cannot assign to '{0}' because it is a function.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_NAMESPACE: &str =
        "Cannot assign to '{0}' because it is a namespace.";
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_IMPORT: &str =
        "Cannot assign to '{0}' because it is an import.";
    pub const JSX_PROPERTY_ACCESS_EXPRESSIONS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES: &str =
        "JSX property access expressions cannot include JSX namespace names";
    pub const INDEX_SIGNATURES_ARE_INCOMPATIBLE: &str = "'{0}' index signatures are incompatible.";
    pub const TYPE_HAS_NO_SIGNATURES_FOR_WHICH_THE_TYPE_ARGUMENT_LIST_IS_APPLICABLE: &str =
        "Type '{0}' has no signatures for which the type argument list is applicable.";
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_IMPLIED_BY_VARIANCE_ANNOTATION: &str =
        "Type '{0}' is not assignable to type '{1}' as implied by variance annotation.";
    pub const VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS:
        &str = "Variance annotations are only supported in type aliases for object, function, constructor, and mapped types.";
    pub const TYPE_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND:
        &str = "Type '{0}' may represent a primitive value, which is not permitted as the right operand of the 'in' operator.";
    pub const REACT_COMPONENTS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES: &str =
        "React components cannot include JSX namespace names";
    pub const CANNOT_AUGMENT_MODULE_WITH_VALUE_EXPORTS_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENT:
        &str = "Cannot augment module '{0}' with value exports because it resolves to a non-module entity.";
    pub const NON_ABSTRACT_CLASS_EXPRESSION_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBE:
        &str = "Non-abstract class expression is missing implementations for the following members of '{0}': {1} and {2} more.";
    pub const A_MEMBER_INITIALIZER_IN_A_ENUM_DECLARATION_CANNOT_REFERENCE_MEMBERS_DECLARED_AFT:
        &str = "A member initializer in a enum declaration cannot reference members declared after it, including members defined in other enums.";
    pub const MERGED_DECLARATION_CANNOT_INCLUDE_A_DEFAULT_EXPORT_DECLARATION_CONSIDER_ADDING_A:
        &str = "Merged declaration '{0}' cannot include a default export declaration. Consider adding a separate 'export default {0}' declaration instead.";
    pub const NON_ABSTRACT_CLASS_EXPRESSION_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM:
        &str = "Non-abstract class expression does not implement inherited abstract member '{0}' from class '{1}'.";
    pub const NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF: &str = "Non-abstract class '{0}' is missing implementations for the following members of '{1}': {2}.";
    pub const NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF_AND_M:
        &str = "Non-abstract class '{0}' is missing implementations for the following members of '{1}': {2} and {3} more.";
    pub const NON_ABSTRACT_CLASS_EXPRESSION_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBE_2:
        &str = "Non-abstract class expression is missing implementations for the following members of '{0}': {1}.";
    pub const JSX_EXPRESSIONS_MUST_HAVE_ONE_PARENT_ELEMENT: &str =
        "JSX expressions must have one parent element.";
    pub const TYPE_PROVIDES_NO_MATCH_FOR_THE_SIGNATURE: &str =
        "Type '{0}' provides no match for the signature '{1}'.";
    pub const SUPER_IS_ONLY_ALLOWED_IN_MEMBERS_OF_OBJECT_LITERAL_EXPRESSIONS_WHEN_OPTION_TARGE:
        &str = "'super' is only allowed in members of object literal expressions when option 'target' is 'ES2015' or higher.";
    pub const SUPER_CAN_ONLY_BE_REFERENCED_IN_MEMBERS_OF_DERIVED_CLASSES_OR_OBJECT_LITERAL_EXP:
        &str = "'super' can only be referenced in members of derived classes or object literal expressions.";
    pub const CANNOT_EXPORT_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE: &str =
        "Cannot export '{0}'. Only local declarations can be exported from a module.";
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER: &str =
        "Cannot find name '{0}'. Did you mean the static member '{1}.{0}'?";
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS: &str =
        "Cannot find name '{0}'. Did you mean the instance member 'this.{0}'?";
    pub const INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND: &str =
        "Invalid module name in augmentation, module '{0}' cannot be found.";
    pub const INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_RESOLVES_TO_AN_UNTYPED_MODULE_AT_WHIC:
        &str = "Invalid module name in augmentation. Module '{0}' resolves to an untyped module at '{1}', which cannot be augmented.";
    pub const EXPORTS_AND_EXPORT_ASSIGNMENTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS: &str =
        "Exports and export assignments are not permitted in module augmentations.";
    pub const IMPORTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS_CONSIDER_MOVING_THEM_TO_THE_EN:
        &str = "Imports are not permitted in module augmentations. Consider moving them to the enclosing external module.";
    pub const EXPORT_MODIFIER_CANNOT_BE_APPLIED_TO_AMBIENT_MODULES_AND_MODULE_AUGMENTATIONS_SI:
        &str = "'export' modifier cannot be applied to ambient modules and module augmentations since they are always visible.";
    pub const AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL:
        &str = "Augmentations for the global scope can only be directly nested in external modules or ambient module declarations.";
    pub const AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_SHOULD_HAVE_DECLARE_MODIFIER_UNLESS_THEY_APPE:
        &str = "Augmentations for the global scope should have 'declare' modifier unless they appear in already ambient context.";
    pub const CANNOT_AUGMENT_MODULE_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENTITY: &str =
        "Cannot augment module '{0}' because it resolves to a non-module entity.";
    pub const CANNOT_ASSIGN_A_CONSTRUCTOR_TYPE_TO_A_CONSTRUCTOR_TYPE: &str =
        "Cannot assign a '{0}' constructor type to a '{1}' constructor type.";
    pub const CONSTRUCTOR_OF_CLASS_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATION:
        &str =
        "Constructor of class '{0}' is private and only accessible within the class declaration.";
    pub const CONSTRUCTOR_OF_CLASS_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATI:
        &str =
        "Constructor of class '{0}' is protected and only accessible within the class declaration.";
    pub const CANNOT_EXTEND_A_CLASS_CLASS_CONSTRUCTOR_IS_MARKED_AS_PRIVATE: &str =
        "Cannot extend a class '{0}'. Class constructor is marked as private.";
    pub const ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT: &str =
        "Accessors must both be abstract or non-abstract.";
    pub const A_TYPE_PREDICATES_TYPE_MUST_BE_ASSIGNABLE_TO_ITS_PARAMETERS_TYPE: &str =
        "A type predicate's type must be assignable to its parameter's type.";
    pub const TYPE_IS_NOT_COMPARABLE_TO_TYPE: &str = "Type '{0}' is not comparable to type '{1}'.";
    pub const A_FUNCTION_THAT_IS_CALLED_WITH_THE_NEW_KEYWORD_CANNOT_HAVE_A_THIS_TYPE_THAT_IS_V:
        &str = "A function that is called with the 'new' keyword cannot have a 'this' type that is 'void'.";
    pub const A_PARAMETER_MUST_BE_THE_FIRST_PARAMETER: &str =
        "A '{0}' parameter must be the first parameter.";
    pub const A_CONSTRUCTOR_CANNOT_HAVE_A_THIS_PARAMETER: &str =
        "A constructor cannot have a 'this' parameter.";
    pub const THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION: &str =
        "'this' implicitly has type 'any' because it does not have a type annotation.";
    pub const THE_THIS_CONTEXT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_METHODS_THIS_OF_TYPE: &str =
        "The 'this' context of type '{0}' is not assignable to method's 'this' of type '{1}'.";
    pub const THE_THIS_TYPES_OF_EACH_SIGNATURE_ARE_INCOMPATIBLE: &str =
        "The 'this' types of each signature are incompatible.";
    pub const REFERS_TO_A_UMD_GLOBAL_BUT_THE_CURRENT_FILE_IS_A_MODULE_CONSIDER_ADDING_AN_IMPOR:
        &str = "'{0}' refers to a UMD global, but the current file is a module. Consider adding an import instead.";
    pub const ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS: &str =
        "All declarations of '{0}' must have identical modifiers.";
    pub const CANNOT_FIND_TYPE_DEFINITION_FILE_FOR: &str =
        "Cannot find type definition file for '{0}'.";
    pub const CANNOT_EXTEND_AN_INTERFACE_DID_YOU_MEAN_IMPLEMENTS: &str =
        "Cannot extend an interface '{0}'. Did you mean 'implements'?";
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DID_YOU_MEAN_TO_USE_IN: &str = "'{0}' only refers to a type, but is being used as a value here. Did you mean to use '{1} in {0}'?";
    pub const IS_A_PRIMITIVE_BUT_IS_A_WRAPPER_OBJECT_PREFER_USING_WHEN_POSSIBLE: &str =
        "'{0}' is a primitive, but '{1}' is a wrapper object. Prefer using '{0}' when possible.";
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE: &str =
        "'{0}' only refers to a type, but is being used as a value here.";
    pub const NAMESPACE_HAS_NO_EXPORTED_MEMBER: &str =
        "Namespace '{0}' has no exported member '{1}'.";
    pub const LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS: &str =
        "Left side of comma operator is unused and has no side effects.";
    pub const THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN:
        &str = "The 'Object' type is assignable to very few other types. Did you mean to use the 'any' type instead?";
    pub const AN_ASYNC_FUNCTION_OR_METHOD_MUST_RETURN_A_PROMISE_MAKE_SURE_YOU_HAVE_A_DECLARATI:
        &str = "An async function or method must return a 'Promise'. Make sure you have a declaration for 'Promise' or include 'ES2015' in your '--lib' option.";
    pub const SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES: &str =
        "Spread types may only be created from object types.";
    pub const STATIC_PROPERTY_CONFLICTS_WITH_BUILT_IN_PROPERTY_FUNCTION_OF_CONSTRUCTOR_FUNCTIO:
        &str = "Static property '{0}' conflicts with built-in property 'Function.{0}' of constructor function '{1}'.";
    pub const REST_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES: &str =
        "Rest types may only be created from object types.";
    pub const THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS:
        &str = "The target of an object rest assignment must be a variable or a property access.";
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_NAMESPACE_HERE: &str =
        "'{0}' only refers to a type, but is being used as a namespace here.";
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE: &str =
        "The operand of a 'delete' operator must be a property reference.";
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_READ_ONLY_PROPERTY: &str =
        "The operand of a 'delete' operator cannot be a read-only property.";
    pub const AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO:
        &str = "An async function or method in ES5 requires the 'Promise' constructor.  Make sure you have a declaration for the 'Promise' constructor or include 'ES2015' in your '--lib' option.";
    pub const REQUIRED_TYPE_PARAMETERS_MAY_NOT_FOLLOW_OPTIONAL_TYPE_PARAMETERS: &str =
        "Required type parameters may not follow optional type parameters.";
    pub const GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS: &str =
        "Generic type '{0}' requires between {1} and {2} type arguments.";
    pub const CANNOT_USE_NAMESPACE_AS_A_VALUE: &str = "Cannot use namespace '{0}' as a value.";
    pub const CANNOT_USE_NAMESPACE_AS_A_TYPE: &str = "Cannot use namespace '{0}' as a type.";
    pub const ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN: &str =
        "'{0}' are specified twice. The attribute named '{0}' will be overwritten.";
    pub const A_DYNAMIC_IMPORT_CALL_RETURNS_A_PROMISE_MAKE_SURE_YOU_HAVE_A_DECLARATION_FOR_PRO:
        &str = "A dynamic import call returns a 'Promise'. Make sure you have a declaration for 'Promise' or include 'ES2015' in your '--lib' option.";
    pub const A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE:
        &str = "A dynamic import call in ES5 requires the 'Promise' constructor.  Make sure you have a declaration for the 'Promise' constructor or include 'ES2015' in your '--lib' option.";
    pub const CANNOT_ACCESS_BECAUSE_IS_A_TYPE_BUT_NOT_A_NAMESPACE_DID_YOU_MEAN_TO_RETRIEVE_THE:
        &str = "Cannot access '{0}.{1}' because '{0}' is a type, but not a namespace. Did you mean to retrieve the type of the property '{1}' in '{0}' with '{0}[\"{1}\"]'?";
    pub const THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I:
        &str = "The expression of an export assignment must be an identifier or qualified name in an ambient context.";
    pub const ABSTRACT_PROPERTY_IN_CLASS_CANNOT_BE_ACCESSED_IN_THE_CONSTRUCTOR: &str =
        "Abstract property '{0}' in class '{1}' cannot be accessed in the constructor.";
    pub const TYPE_PARAMETER_HAS_A_CIRCULAR_DEFAULT: &str =
        "Type parameter '{0}' has a circular default.";
    pub const SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP:
        &str = "Subsequent property declarations must have the same type.  Property '{0}' must be of type '{1}', but here has type '{2}'.";
    pub const DUPLICATE_PROPERTY: &str = "Duplicate property '{0}'.";
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY:
        &str = "Type '{0}' is not assignable to type '{1}'. Two different types with this name exist, but they are unrelated.";
    pub const CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER:
        &str = "Class '{0}' incorrectly implements class '{1}'. Did you mean to extend '{1}' and inherit its members as a subclass?";
    pub const CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL: &str =
        "Cannot invoke an object which is possibly 'null'.";
    pub const CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED: &str =
        "Cannot invoke an object which is possibly 'undefined'.";
    pub const CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL_OR_UNDEFINED: &str =
        "Cannot invoke an object which is possibly 'null' or 'undefined'.";
    pub const HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN: &str =
        "'{0}' has no exported member named '{1}'. Did you mean '{2}'?";
    pub const CLASS_NAME_CANNOT_BE_OBJECT_WHEN_TARGETING_ES5_AND_ABOVE_WITH_MODULE: &str =
        "Class name cannot be 'Object' when targeting ES5 and above with module {0}.";
    pub const CANNOT_FIND_LIB_DEFINITION_FOR: &str = "Cannot find lib definition for '{0}'.";
    pub const CANNOT_FIND_LIB_DEFINITION_FOR_DID_YOU_MEAN: &str =
        "Cannot find lib definition for '{0}'. Did you mean '{1}'?";
    pub const IS_DECLARED_HERE: &str = "'{0}' is declared here.";
    pub const PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION: &str =
        "Property '{0}' is used before its initialization.";
    pub const AN_ARROW_FUNCTION_CANNOT_HAVE_A_THIS_PARAMETER: &str =
        "An arrow function cannot have a 'this' parameter.";
    pub const IMPLICIT_CONVERSION_OF_A_SYMBOL_TO_A_STRING_WILL_FAIL_AT_RUNTIME_CONSIDER_WRAPPI:
        &str = "Implicit conversion of a 'symbol' to a 'string' will fail at runtime. Consider wrapping this expression in 'String(...)'.";
    pub const CANNOT_FIND_MODULE_CONSIDER_USING_RESOLVEJSONMODULE_TO_IMPORT_MODULE_WITH_JSON_E:
        &str = "Cannot find module '{0}'. Consider using '--resolveJsonModule' to import module with '.json' extension.";
    pub const PROPERTY_WAS_ALSO_DECLARED_HERE: &str = "Property '{0}' was also declared here.";
    pub const ARE_YOU_MISSING_A_SEMICOLON: &str = "Are you missing a semicolon?";
    pub const DID_YOU_MEAN_FOR_TO_BE_CONSTRAINED_TO_TYPE_NEW_ARGS_ANY: &str =
        "Did you mean for '{0}' to be constrained to type 'new (...args: any[]) => {1}'?";
    pub const OPERATOR_CANNOT_BE_APPLIED_TO_TYPE: &str =
        "Operator '{0}' cannot be applied to type '{1}'.";
    pub const BIGINT_LITERALS_ARE_NOT_AVAILABLE_WHEN_TARGETING_LOWER_THAN_ES2020: &str =
        "BigInt literals are not available when targeting lower than ES2020.";
    pub const AN_OUTER_VALUE_OF_THIS_IS_SHADOWED_BY_THIS_CONTAINER: &str =
        "An outer value of 'this' is shadowed by this container.";
    pub const TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE: &str =
        "Type '{0}' is missing the following properties from type '{1}': {2}";
    pub const TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE: &str =
        "Type '{0}' is missing the following properties from type '{1}': {2}, and {3} more.";
    pub const PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE: &str =
        "Property '{0}' is missing in type '{1}' but required in type '{2}'.";
    pub const THE_INFERRED_TYPE_OF_CANNOT_BE_NAMED_WITHOUT_A_REFERENCE_TO_THIS_IS_LIKELY_NOT_P:
        &str = "The inferred type of '{0}' cannot be named without a reference to '{1}'. This is likely not portable. A type annotation is necessary.";
    pub const NO_OVERLOAD_EXPECTS_TYPE_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR:
        &str = "No overload expects {0} type arguments, but overloads do exist that expect either {1} or {2} type arguments.";
    pub const TYPE_PARAMETER_DEFAULTS_CAN_ONLY_REFERENCE_PREVIOUSLY_DECLARED_TYPE_PARAMETERS: &str =
        "Type parameter defaults can only reference previously declared type parameters.";
    pub const THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING:
        &str = "This JSX tag's '{0}' prop expects type '{1}' which requires multiple children, but only a single child was provided.";
    pub const THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO:
        &str = "This JSX tag's '{0}' prop expects a single child of type '{1}', but multiple children were provided.";
    pub const COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU:
        &str = "'{0}' components don't accept text as child elements. Text in JSX has the type 'string', but the expected type of '{1}' is '{2}'.";
    pub const CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED: &str =
        "Cannot access ambient const enums when '{0}' is enabled.";
    pub const REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF: &str =
        "'{0}' refers to a value, but is being used as a type here. Did you mean 'typeof {0}'?";
    pub const THE_IMPLEMENTATION_SIGNATURE_IS_DECLARED_HERE: &str =
        "The implementation signature is declared here.";
    pub const CIRCULARITY_ORIGINATES_IN_TYPE_AT_THIS_LOCATION: &str =
        "Circularity originates in type at this location.";
    pub const THE_FIRST_EXPORT_DEFAULT_IS_HERE: &str = "The first export default is here.";
    pub const ANOTHER_EXPORT_DEFAULT_IS_HERE: &str = "Another export default is here.";
    pub const SUPER_MAY_NOT_USE_TYPE_ARGUMENTS: &str = "'super' may not use type arguments.";
    pub const NO_CONSTITUENT_OF_TYPE_IS_CALLABLE: &str =
        "No constituent of type '{0}' is callable.";
    pub const NOT_ALL_CONSTITUENTS_OF_TYPE_ARE_CALLABLE: &str =
        "Not all constituents of type '{0}' are callable.";
    pub const TYPE_HAS_NO_CALL_SIGNATURES: &str = "Type '{0}' has no call signatures.";
    pub const EACH_MEMBER_OF_THE_UNION_TYPE_HAS_SIGNATURES_BUT_NONE_OF_THOSE_SIGNATURES_ARE_CO:
        &str = "Each member of the union type '{0}' has signatures, but none of those signatures are compatible with each other.";
    pub const NO_CONSTITUENT_OF_TYPE_IS_CONSTRUCTABLE: &str =
        "No constituent of type '{0}' is constructable.";
    pub const NOT_ALL_CONSTITUENTS_OF_TYPE_ARE_CONSTRUCTABLE: &str =
        "Not all constituents of type '{0}' are constructable.";
    pub const TYPE_HAS_NO_CONSTRUCT_SIGNATURES: &str = "Type '{0}' has no construct signatures.";
    pub const EACH_MEMBER_OF_THE_UNION_TYPE_HAS_CONSTRUCT_SIGNATURES_BUT_NONE_OF_THOSE_SIGNATU:
        &str = "Each member of the union type '{0}' has construct signatures, but none of those signatures are compatible with each other.";
    pub const CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_FO:
        &str = "Cannot iterate value because the 'next' method of its iterator expects type '{1}', but for-of will always send '{0}'.";
    pub const CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR:
        &str = "Cannot iterate value because the 'next' method of its iterator expects type '{1}', but array spread will always send '{0}'.";
    pub const CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR_2:
        &str = "Cannot iterate value because the 'next' method of its iterator expects type '{1}', but array destructuring will always send '{0}'.";
    pub const CANNOT_DELEGATE_ITERATION_TO_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPEC:
        &str = "Cannot delegate iteration to value because the 'next' method of its iterator expects type '{1}', but the containing generator will always send '{0}'.";
    pub const THE_PROPERTY_OF_AN_ITERATOR_MUST_BE_A_METHOD: &str =
        "The '{0}' property of an iterator must be a method.";
    pub const THE_PROPERTY_OF_AN_ASYNC_ITERATOR_MUST_BE_A_METHOD: &str =
        "The '{0}' property of an async iterator must be a method.";
    pub const NO_OVERLOAD_MATCHES_THIS_CALL: &str = "No overload matches this call.";
    pub const THE_LAST_OVERLOAD_GAVE_THE_FOLLOWING_ERROR: &str =
        "The last overload gave the following error.";
    pub const THE_LAST_OVERLOAD_IS_DECLARED_HERE: &str = "The last overload is declared here.";
    pub const OVERLOAD_OF_GAVE_THE_FOLLOWING_ERROR: &str =
        "Overload {0} of {1}, '{2}', gave the following error.";
    pub const DID_YOU_FORGET_TO_USE_AWAIT: &str = "Did you forget to use 'await'?";
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_FUNCTION_IS_ALWAYS_DEFINED_DID:
        &str = "This condition will always return true since this function is always defined. Did you mean to call it instead?";
    pub const ASSERTIONS_REQUIRE_EVERY_NAME_IN_THE_CALL_TARGET_TO_BE_DECLARED_WITH_AN_EXPLICIT:
        &str = "Assertions require every name in the call target to be declared with an explicit type annotation.";
    pub const ASSERTIONS_REQUIRE_THE_CALL_TARGET_TO_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME: &str =
        "Assertions require the call target to be an identifier or qualified name.";
    pub const THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERT:
        &str =
        "The operand of an increment or decrement operator may not be an optional property access.";
    pub const THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS: &str =
        "The target of an object rest assignment may not be an optional property access.";
    pub const THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A:
        &str =
        "The left-hand side of an assignment expression may not be an optional property access.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS:
        &str =
        "The left-hand side of a 'for...in' statement may not be an optional property access.";
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS:
        &str =
        "The left-hand side of a 'for...of' statement may not be an optional property access.";
    pub const NEEDS_AN_EXPLICIT_TYPE_ANNOTATION: &str = "'{0}' needs an explicit type annotation.";
    pub const IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN: &str =
        "'{0}' is specified more than once, so this usage will be overwritten.";
    pub const GET_AND_SET_ACCESSORS_CANNOT_DECLARE_THIS_PARAMETERS: &str =
        "'get' and 'set' accessors cannot declare 'this' parameters.";
    pub const THIS_SPREAD_ALWAYS_OVERWRITES_THIS_PROPERTY: &str =
        "This spread always overwrites this property.";
    pub const CANNOT_BE_USED_AS_A_JSX_COMPONENT: &str = "'{0}' cannot be used as a JSX component.";
    pub const ITS_RETURN_TYPE_IS_NOT_A_VALID_JSX_ELEMENT: &str =
        "Its return type '{0}' is not a valid JSX element.";
    pub const ITS_INSTANCE_TYPE_IS_NOT_A_VALID_JSX_ELEMENT: &str =
        "Its instance type '{0}' is not a valid JSX element.";
    pub const ITS_ELEMENT_TYPE_IS_NOT_A_VALID_JSX_ELEMENT: &str =
        "Its element type '{0}' is not a valid JSX element.";
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL: &str =
        "The operand of a 'delete' operator must be optional.";
    pub const EXPONENTIATION_CANNOT_BE_PERFORMED_ON_BIGINT_VALUES_UNLESS_THE_TARGET_OPTION_IS:
        &str = "Exponentiation cannot be performed on 'bigint' values unless the 'target' option is set to 'es2016' or later.";
    pub const CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O:
        &str = "Cannot find module '{0}'. Did you mean to set the 'moduleResolution' option to 'nodenext', or to add aliases to the 'paths' option?";
    pub const THE_CALL_WOULD_HAVE_SUCCEEDED_AGAINST_THIS_IMPLEMENTATION_BUT_IMPLEMENTATION_SIG:
        &str = "The call would have succeeded against this implementation, but implementation signatures of overloads are not externally visible.";
    pub const EXPECTED_ARGUMENTS_BUT_GOT_DID_YOU_FORGET_TO_INCLUDE_VOID_IN_YOUR_TYPE_ARGUMENT:
        &str = "Expected {0} arguments, but got {1}. Did you forget to include 'void' in your type argument to 'Promise'?";
    pub const THE_INTRINSIC_KEYWORD_CAN_ONLY_BE_USED_TO_DECLARE_COMPILER_PROVIDED_INTRINSIC_TY:
        &str =
        "The 'intrinsic' keyword can only be used to declare compiler provided intrinsic types.";
    pub const IT_IS_LIKELY_THAT_YOU_ARE_MISSING_A_COMMA_TO_SEPARATE_THESE_TWO_TEMPLATE_EXPRESS:
        &str = "It is likely that you are missing a comma to separate these two template expressions. They form a tagged template expression which cannot be invoked.";
    pub const A_MIXIN_CLASS_THAT_EXTENDS_FROM_A_TYPE_VARIABLE_CONTAINING_AN_ABSTRACT_CONSTRUCT:
        &str = "A mixin class that extends from a type variable containing an abstract construct signature must also be declared 'abstract'.";
    pub const THE_DECLARATION_WAS_MARKED_AS_DEPRECATED_HERE: &str =
        "The declaration was marked as deprecated here.";
    pub const TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT: &str =
        "Type produces a tuple type that is too large to represent.";
    pub const EXPRESSION_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT: &str =
        "Expression produces a tuple type that is too large to represent.";
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_IS_ALWAYS_DEFINED: &str =
        "This condition will always return true since this '{0}' is always defined.";
    pub const TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH:
        &str = "Type '{0}' can only be iterated through when using the '--downlevelIteration' flag or with a '--target' of 'es2015' or higher.";
    pub const CANNOT_ASSIGN_TO_PRIVATE_METHOD_PRIVATE_METHODS_ARE_NOT_WRITABLE: &str =
        "Cannot assign to private method '{0}'. Private methods are not writable.";
    pub const DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE:
        &str = "Duplicate identifier '{0}'. Static and instance elements cannot share the same private name.";
    pub const PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER: &str =
        "Private accessor was defined without a getter.";
    pub const THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WITH_PARAMETERS_WHICH_IS_NOT_COMPA:
        &str = "This syntax requires an imported helper named '{1}' with {2} parameters, which is not compatible with the one in '{0}'. Consider upgrading your version of '{0}'.";
    pub const A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER: &str =
        "A get accessor must be at least as accessible as the setter";
    pub const DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I:
        &str = "Declaration or statement expected. This '=' follows a block of statements, so if you intended to write a destructuring assignment, you might need to wrap the whole assignment in parentheses.";
    pub const EXPECTED_1_ARGUMENT_BUT_GOT_0_NEW_PROMISE_NEEDS_A_JSDOC_HINT_TO_PRODUCE_A_RESOLV:
        &str = "Expected 1 argument, but got 0. 'new Promise()' needs a JSDoc hint to produce a 'resolve' that can be called without arguments.";
    pub const INITIALIZER_FOR_PROPERTY: &str = "Initializer for property '{0}'";
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_INCLUDE:
        &str = "Property '{0}' does not exist on type '{1}'. Try changing the 'lib' compiler option to include 'dom'.";
    pub const CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR: &str =
        "Class declaration cannot implement overload list for '{0}'.";
    pub const FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT: &str =
        "Function with bodies can only merge with classes that are ambient.";
    pub const ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI:
        &str = "'arguments' cannot be referenced in property initializers or class static initialization blocks.";
    pub const CANNOT_USE_THIS_IN_A_STATIC_PROPERTY_INITIALIZER_OF_A_DECORATED_CLASS: &str =
        "Cannot use 'this' in a static property initializer of a decorated class.";
    pub const PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_A_CLASS_STATIC_BLO:
        &str =
        "Property '{0}' has no initializer and is not definitely assigned in a class static block.";
    pub const DUPLICATE_IDENTIFIER_COMPILER_RESERVES_NAME_WHEN_EMITTING_SUPER_REFERENCES_IN_ST:
        &str = "Duplicate identifier '{0}'. Compiler reserves name '{1}' when emitting 'super' references in static initializers.";
    pub const NAMESPACE_NAME_CANNOT_BE: &str = "Namespace name cannot be '{0}'.";
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN: &str =
        "Type '{0}' is not assignable to type '{1}'. Did you mean '{2}'?";
    pub const IMPORT_ASSERTIONS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD:
        &str = "Import assertions are only supported when the '--module' option is set to 'esnext', 'node18', 'node20', 'nodenext', or 'preserve'.";
    pub const IMPORT_ASSERTIONS_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS: &str =
        "Import assertions cannot be used with type-only imports or exports.";
    pub const IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD:
        &str = "Import attributes are only supported when the '--module' option is set to 'esnext', 'node18', 'node20', 'nodenext', or 'preserve'.";
    pub const CANNOT_FIND_NAMESPACE_DID_YOU_MEAN: &str =
        "Cannot find namespace '{0}'. Did you mean '{1}'?";
    pub const RELATIVE_IMPORT_PATHS_NEED_EXPLICIT_FILE_EXTENSIONS_IN_ECMASCRIPT_IMPORTS_WHEN_M:
        &str = "Relative import paths need explicit file extensions in ECMAScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Consider adding an extension to the import path.";
    pub const RELATIVE_IMPORT_PATHS_NEED_EXPLICIT_FILE_EXTENSIONS_IN_ECMASCRIPT_IMPORTS_WHEN_M_2:
        &str = "Relative import paths need explicit file extensions in ECMAScript imports when '--moduleResolution' is 'node16' or 'nodenext'. Did you mean '{0}'?";
    pub const IMPORT_ASSERTIONS_ARE_NOT_ALLOWED_ON_STATEMENTS_THAT_COMPILE_TO_COMMONJS_REQUIRE:
        &str =
        "Import assertions are not allowed on statements that compile to CommonJS 'require' calls.";
    pub const IMPORT_ASSERTION_VALUES_MUST_BE_STRING_LITERAL_EXPRESSIONS: &str =
        "Import assertion values must be string literal expressions.";
    pub const ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_CONSTRAINTS: &str =
        "All declarations of '{0}' must have identical constraints.";
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN_SINCE_JAVASCRIPT_COMPARES_OBJECTS_BY_REFERENCE:
        &str = "This condition will always return '{0}' since JavaScript compares objects by reference, not value.";
    pub const AN_INTERFACE_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_EXTEND_OTHER_NAMED:
        &str = "An interface cannot extend a primitive type like '{0}'. It can only extend other named object types.";
    pub const IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION: &str =
        "'{0}' is an unused renaming of '{1}'. Did you intend to use it as a type annotation?";
    pub const WE_CAN_ONLY_WRITE_A_TYPE_FOR_BY_ADDING_A_TYPE_FOR_THE_ENTIRE_PARAMETER_HERE: &str =
        "We can only write a type for '{0}' by adding a type for the entire parameter here.";
    pub const TYPE_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN_THE_CON:
        &str = "Type of instance member variable '{0}' cannot reference identifier '{1}' declared in the constructor.";
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN: &str = "This condition will always return '{0}'.";
    pub const A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT:
        &str = "A declaration file cannot be imported without 'import type'. Did you mean to import an implementation file '{0}' instead?";
    pub const THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP:
        &str = "The right-hand side of an 'instanceof' expression must not be an instantiation expression.";
    pub const TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT: &str =
        "Target signature provides too few arguments. Expected {0} or more, but got {1}.";
    pub const THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI:
        &str = "The initializer of a 'using' declaration must be either an object with a '[Symbol.dispose]()' method, or be 'null' or 'undefined'.";
    pub const THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY:
        &str = "The initializer of an 'await using' declaration must be either an object with a '[Symbol.asyncDispose]()' or '[Symbol.dispose]()' method, or be 'null' or 'undefined'.";
    pub const AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE:
        &str = "'await using' statements are only allowed within async functions and at the top levels of modules.";
    pub const AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FIL:
        &str = "'await using' statements are only allowed at the top level of a file when that file is a module, but this file has no imports or exports. Consider adding an empty 'export {}' to make this file a module.";
    pub const TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET:
        &str = "Top-level 'await using' statements are only allowed when the 'module' option is set to 'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', 'nodenext', or 'preserve', and the 'target' option is set to 'es2017' or higher.";
    pub const CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA:
        &str = "Class field '{0}' defined by the parent class is not accessible in the child class via super.";
    pub const IMPORT_ATTRIBUTES_ARE_NOT_ALLOWED_ON_STATEMENTS_THAT_COMPILE_TO_COMMONJS_REQUIRE:
        &str =
        "Import attributes are not allowed on statements that compile to CommonJS 'require' calls.";
    pub const IMPORT_ATTRIBUTES_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS: &str =
        "Import attributes cannot be used with type-only imports or exports.";
    pub const IMPORT_ATTRIBUTE_VALUES_MUST_BE_STRING_LITERAL_EXPRESSIONS: &str =
        "Import attribute values must be string literal expressions.";
    pub const EXCESSIVE_COMPLEXITY_COMPARING_TYPES_AND: &str =
        "Excessive complexity comparing types '{0}' and '{1}'.";
    pub const THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_ASSIGNABLE_TO_THE_FIRST_A:
        &str = "The left-hand side of an 'instanceof' expression must be assignable to the first argument of the right-hand side's '[Symbol.hasInstance]' method.";
    pub const AN_OBJECTS_SYMBOL_HASINSTANCE_METHOD_MUST_RETURN_A_BOOLEAN_VALUE_FOR_IT_TO_BE_US:
        &str = "An object's '[Symbol.hasInstance]' method must return a boolean value for it to be used on the right-hand side of an 'instanceof' expression.";
    pub const TYPE_IS_GENERIC_AND_CAN_ONLY_BE_INDEXED_FOR_READING: &str =
        "Type '{0}' is generic and can only be indexed for reading.";
    pub const A_CLASS_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_CLASSES_CAN_ONLY_EXTEND_CONSTRUCTABL:
        &str = "A class cannot extend a primitive type like '{0}'. Classes can only extend constructable values.";
    pub const A_CLASS_CANNOT_IMPLEMENT_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_IMPLEMENT_OTHER_NAMED:
        &str = "A class cannot implement a primitive type like '{0}'. It can only implement other named object types.";
    pub const IMPORT_CONFLICTS_WITH_LOCAL_VALUE_SO_MUST_BE_DECLARED_WITH_A_TYPE_ONLY_IMPORT_WH:
        &str = "Import '{0}' conflicts with local value, so must be declared with a type-only import when 'isolatedModules' is enabled.";
    pub const IMPORT_CONFLICTS_WITH_GLOBAL_VALUE_USED_IN_THIS_FILE_SO_MUST_BE_DECLARED_WITH_A:
        &str = "Import '{0}' conflicts with global value used in this file, so must be declared with a type-only import when 'isolatedModules' is enabled.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for Bun? Try `npm i --save-dev @types/bun`.";
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE_2:
        &str = "Cannot find name '{0}'. Do you need to install type definitions for Bun? Try `npm i --save-dev @types/bun` and then add 'bun' to the types field in your tsconfig.";
    pub const RIGHT_OPERAND_OF_IS_UNREACHABLE_BECAUSE_THE_LEFT_OPERAND_IS_NEVER_NULLISH: &str =
        "Right operand of ?? is unreachable because the left operand is never nullish.";
    pub const THIS_BINARY_EXPRESSION_IS_NEVER_NULLISH_ARE_YOU_MISSING_PARENTHESES: &str =
        "This binary expression is never nullish. Are you missing parentheses?";
    pub const THIS_EXPRESSION_IS_ALWAYS_NULLISH: &str = "This expression is always nullish.";
    pub const THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY: &str =
        "This kind of expression is always truthy.";
    pub const THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY: &str =
        "This kind of expression is always falsy.";
    pub const THIS_JSX_TAG_REQUIRES_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND: &str =
        "This JSX tag requires '{0}' to be in scope, but it could not be found.";
    pub const THIS_JSX_TAG_REQUIRES_THE_MODULE_PATH_TO_EXIST_BUT_NONE_COULD_BE_FOUND_MAKE_SURE:
        &str = "This JSX tag requires the module path '{0}' to exist, but none could be found. Make sure you have types for the appropriate package installed.";
    pub const THIS_RELATIVE_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_LOOKS_LIKE_A_FILE_NAME:
        &str = "This relative import path is unsafe to rewrite because it looks like a file name, but actually resolves to \"{0}\".";
    pub const THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT:
        &str = "This import uses a '{0}' extension to resolve to an input TypeScript file, but will not be rewritten during emit because it is not a relative path.";
    pub const THIS_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_RESOLVES_TO_ANOTHER_PROJECT_AND:
        &str = "This import path is unsafe to rewrite because it resolves to another project, and the relative path between the projects' output files is not the same as the relative path between its input files.";
    pub const USING_JSX_FRAGMENTS_REQUIRES_FRAGMENT_FACTORY_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE:
        &str = "Using JSX fragments requires fragment factory '{0}' to be in scope, but it could not be found.";
    pub const IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS:
        &str = "Import assertions have been replaced by import attributes. Use 'with' instead of 'assert'.";
    pub const THIS_EXPRESSION_IS_NEVER_NULLISH: &str = "This expression is never nullish.";
    pub const CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF: &str =
        "Cannot find module or type declarations for side-effect import of '{0}'.";
    pub const IMPORT_DECLARATION_IS_USING_PRIVATE_NAME: &str =
        "Import declaration '{0}' is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Type parameter '{0}' of exported class has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Type parameter '{0}' of exported interface has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING:
        &str = "Type parameter '{0}' of constructor signature from exported interface has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE:
        &str = "Type parameter '{0}' of call signature from exported interface has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVA:
        &str = "Type parameter '{0}' of public static method from exported class has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME:
        &str = "Type parameter '{0}' of public method from exported class has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Type parameter '{0}' of method from exported interface has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Type parameter '{0}' of exported function has or is using private name '{1}'.";
    pub const IMPLEMENTS_CLAUSE_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Implements clause of exported class '{0}' has or is using private name '{1}'.";
    pub const EXTENDS_CLAUSE_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "'extends' clause of exported class '{0}' has or is using private name '{1}'.";
    pub const EXTENDS_CLAUSE_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_2: &str =
        "'extends' clause of exported class has or is using private name '{0}'.";
    pub const EXTENDS_CLAUSE_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "'extends' clause of exported interface '{0}' has or is using private name '{1}'.";
    pub const EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NAMED:
        &str = "Exported variable '{0}' has or is using name '{1}' from external module {2} but cannot be named.";
    pub const EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str =
        "Exported variable '{0}' has or is using name '{1}' from private module '{2}'.";
    pub const EXPORTED_VARIABLE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Exported variable '{0}' has or is using private name '{1}'.";
    pub const PUBLIC_STATIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODU:
        &str = "Public static property '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PUBLIC_STATIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODUL:
        &str = "Public static property '{0}' of exported class has or is using name '{1}' from private module '{2}'.";
    pub const PUBLIC_STATIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Public static property '{0}' of exported class has or is using private name '{1}'.";
    pub const PUBLIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT:
        &str = "Public property '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PUBLIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str = "Public property '{0}' of exported class has or is using name '{1}' from private module '{2}'.";
    pub const PUBLIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Public property '{0}' of exported class has or is using private name '{1}'.";
    pub const PROPERTY_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str = "Property '{0}' of exported interface has or is using name '{1}' from private module '{2}'.";
    pub const PROPERTY_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Property '{0}' of exported interface has or is using private name '{1}'.";
    pub const PARAMETER_TYPE_OF_PUBLIC_STATIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME:
        &str = "Parameter type of public static setter '{0}' from exported class has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_TYPE_OF_PUBLIC_STATIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVA:
        &str = "Parameter type of public static setter '{0}' from exported class has or is using private name '{1}'.";
    pub const PARAMETER_TYPE_OF_PUBLIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PR:
        &str = "Parameter type of public setter '{0}' from exported class has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_TYPE_OF_PUBLIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME:
        &str = "Parameter type of public setter '{0}' from exported class has or is using private name '{1}'.";
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO:
        &str = "Return type of public static getter '{0}' from exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO_2:
        &str = "Return type of public static getter '{0}' from exported class has or is using name '{1}' from private module '{2}'.";
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE:
        &str = "Return type of public static getter '{0}' from exported class has or is using private name '{1}'.";
    pub const RETURN_TYPE_OF_PUBLIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTER:
        &str = "Return type of public getter '{0}' from exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const RETURN_TYPE_OF_PUBLIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVA:
        &str = "Return type of public getter '{0}' from exported class has or is using name '{1}' from private module '{2}'.";
    pub const RETURN_TYPE_OF_PUBLIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Return type of public getter '{0}' from exported class has or is using private name '{1}'.";
    pub const RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAM:
        &str = "Return type of constructor signature from exported interface has or is using name '{0}' from private module '{1}'.";
    pub const RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRI:
        &str = "Return type of constructor signature from exported interface has or is using private name '{0}'.";
    pub const RETURN_TYPE_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM:
        &str = "Return type of call signature from exported interface has or is using name '{0}' from private module '{1}'.";
    pub const RETURN_TYPE_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NA:
        &str =
        "Return type of call signature from exported interface has or is using private name '{0}'.";
    pub const RETURN_TYPE_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM:
        &str = "Return type of index signature from exported interface has or is using name '{0}' from private module '{1}'.";
    pub const RETURN_TYPE_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_N:
        &str = "Return type of index signature from exported interface has or is using private name '{0}'.";
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO:
        &str = "Return type of public static method from exported class has or is using name '{0}' from external module {1} but cannot be named.";
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO_2:
        &str = "Return type of public static method from exported class has or is using name '{0}' from private module '{1}'.";
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE:
        &str = "Return type of public static method from exported class has or is using private name '{0}'.";
    pub const RETURN_TYPE_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTER:
        &str = "Return type of public method from exported class has or is using name '{0}' from external module {1} but cannot be named.";
    pub const RETURN_TYPE_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVA:
        &str = "Return type of public method from exported class has or is using name '{0}' from private module '{1}'.";
    pub const RETURN_TYPE_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Return type of public method from exported class has or is using private name '{0}'.";
    pub const RETURN_TYPE_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE:
        &str = "Return type of method from exported interface has or is using name '{0}' from private module '{1}'.";
    pub const RETURN_TYPE_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Return type of method from exported interface has or is using private name '{0}'.";
    pub const RETURN_TYPE_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_C:
        &str = "Return type of exported function has or is using name '{0}' from external module {1} but cannot be named.";
    pub const RETURN_TYPE_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str =
        "Return type of exported function has or is using name '{0}' from private module '{1}'.";
    pub const RETURN_TYPE_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Return type of exported function has or is using private name '{0}'.";
    pub const PARAMETER_OF_CONSTRUCTOR_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL:
        &str = "Parameter '{0}' of constructor from exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PARAMETER_OF_CONSTRUCTOR_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_M:
        &str = "Parameter '{0}' of constructor from exported class has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_CONSTRUCTOR_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Parameter '{0}' of constructor from exported class has or is using private name '{1}'.";
    pub const PARAMETER_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME:
        &str = "Parameter '{0}' of constructor signature from exported interface has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVA:
        &str = "Parameter '{0}' of constructor signature from exported interface has or is using private name '{1}'.";
    pub const PARAMETER_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PR:
        &str = "Parameter '{0}' of call signature from exported interface has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME:
        &str = "Parameter '{0}' of call signature from exported interface has or is using private name '{1}'.";
    pub const PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM:
        &str = "Parameter '{0}' of public static method from exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_2:
        &str = "Parameter '{0}' of public static method from exported class has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NA:
        &str = "Parameter '{0}' of public static method from exported class has or is using private name '{1}'.";
    pub const PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNA:
        &str = "Parameter '{0}' of public method from exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE:
        &str = "Parameter '{0}' of public method from exported class has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Parameter '{0}' of public method from exported class has or is using private name '{1}'.";
    pub const PARAMETER_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MO:
        &str = "Parameter '{0}' of method from exported interface has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Parameter '{0}' of method from exported interface has or is using private name '{1}'.";
    pub const PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CAN:
        &str = "Parameter '{0}' of exported function has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str = "Parameter '{0}' of exported function has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Parameter '{0}' of exported function has or is using private name '{1}'.";
    pub const EXPORTED_TYPE_ALIAS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Exported type alias '{0}' has or is using private name '{1}'.";
    pub const DEFAULT_EXPORT_OF_THE_MODULE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Default export of the module has or is using private name '{0}'.";
    pub const TYPE_PARAMETER_OF_EXPORTED_TYPE_ALIAS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Type parameter '{0}' of exported type alias has or is using private name '{1}'.";
    pub const EXPORTED_TYPE_ALIAS_HAS_OR_IS_USING_PRIVATE_NAME_FROM_MODULE: &str =
        "Exported type alias '{0}' has or is using private name '{1}' from module {2}.";
    pub const EXTENDS_CLAUSE_FOR_INFERRED_TYPE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Extends clause for inferred type '{0}' has or is using private name '{1}'.";
    pub const PARAMETER_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_P:
        &str = "Parameter '{0}' of index signature from exported interface has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAM:
        &str = "Parameter '{0}' of index signature from exported interface has or is using private name '{1}'.";
    pub const PROPERTY_OF_EXPORTED_ANONYMOUS_CLASS_TYPE_MAY_NOT_BE_PRIVATE_OR_PROTECTED: &str =
        "Property '{0}' of exported anonymous class type may not be private or protected.";
    pub const PUBLIC_STATIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE:
        &str = "Public static method '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PUBLIC_STATIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE:
        &str = "Public static method '{0}' of exported class has or is using name '{1}' from private module '{2}'.";
    pub const PUBLIC_STATIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Public static method '{0}' of exported class has or is using private name '{1}'.";
    pub const PUBLIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CA:
        &str = "Public method '{0}' of exported class has or is using name '{1}' from external module {2} but cannot be named.";
    pub const PUBLIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str = "Public method '{0}' of exported class has or is using name '{1}' from private module '{2}'.";
    pub const PUBLIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Public method '{0}' of exported class has or is using private name '{1}'.";
    pub const METHOD_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str =
        "Method '{0}' of exported interface has or is using name '{1}' from private module '{2}'.";
    pub const METHOD_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Method '{0}' of exported interface has or is using private name '{1}'.";
    pub const TYPE_PARAMETER_OF_EXPORTED_MAPPED_OBJECT_TYPE_IS_USING_PRIVATE_NAME: &str =
        "Type parameter '{0}' of exported mapped object type is using private name '{1}'.";
    pub const THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE: &str =
        "The type '{0}' is 'readonly' and cannot be assigned to the mutable type '{1}'.";
    pub const PRIVATE_OR_PROTECTED_MEMBER_CANNOT_BE_ACCESSED_ON_A_TYPE_PARAMETER: &str =
        "Private or protected member '{0}' cannot be accessed on a type parameter.";
    pub const PARAMETER_OF_ACCESSOR_HAS_OR_IS_USING_PRIVATE_NAME: &str =
        "Parameter '{0}' of accessor has or is using private name '{1}'.";
    pub const PARAMETER_OF_ACCESSOR_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: &str =
        "Parameter '{0}' of accessor has or is using name '{1}' from private module '{2}'.";
    pub const PARAMETER_OF_ACCESSOR_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NA:
        &str = "Parameter '{0}' of accessor has or is using name '{1}' from external module '{2}' but cannot be named.";
    pub const TYPE_ARGUMENTS_FOR_CIRCULARLY_REFERENCE_THEMSELVES: &str =
        "Type arguments for '{0}' circularly reference themselves.";
    pub const TUPLE_TYPE_ARGUMENTS_CIRCULARLY_REFERENCE_THEMSELVES: &str =
        "Tuple type arguments circularly reference themselves.";
    pub const PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH: &str =
        "Property '{0}' comes from an index signature, so it must be accessed with ['{0}'].";
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N:
        &str = "This member cannot have an 'override' modifier because its containing class '{0}' does not extend another class.";
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B:
        &str = "This member cannot have an 'override' modifier because it is not declared in the base class '{0}'.";
    pub const THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE:
        &str = "This member must have an 'override' modifier because it overrides a member in the base class '{0}'.";
    pub const THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME:
        &str = "This parameter property must have an 'override' modifier because it overrides a member in base class '{0}'.";
    pub const THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH:
        &str = "This member must have an 'override' modifier because it overrides an abstract method that is declared in the base class '{0}'.";
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2:
        &str = "This member cannot have an 'override' modifier because it is not declared in the base class '{0}'. Did you mean '{1}'?";
    pub const THE_TYPE_OF_THIS_NODE_CANNOT_BE_SERIALIZED_BECAUSE_ITS_PROPERTY_CANNOT_BE_SERIAL:
        &str = "The type of this node cannot be serialized because its property '{0}' cannot be serialized.";
    pub const THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES:
        &str = "This member must have a JSDoc comment with an '@override' tag because it overrides a member in the base class '{0}'.";
    pub const THIS_PARAMETER_PROPERTY_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_I:
        &str = "This parameter property must have a JSDoc comment with an '@override' tag because it overrides a member in the base class '{0}'.";
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_CONTAIN:
        &str = "This member cannot have a JSDoc comment with an '@override' tag because its containing class '{0}' does not extend another class.";
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D:
        &str = "This member cannot have a JSDoc comment with an '@override' tag because it is not declared in the base class '{0}'.";
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D_2:
        &str = "This member cannot have a JSDoc comment with an 'override' tag because it is not declared in the base class '{0}'. Did you mean '{1}'?";
    pub const COMPILER_OPTION_OF_VALUE_IS_UNSTABLE_USE_NIGHTLY_TYPESCRIPT_TO_SILENCE_THIS_ERRO:
        &str = "Compiler option '{0}' of value '{1}' is unstable. Use nightly TypeScript to silence this error. Try updating with 'npm install -D typescript@next'.";
    pub const EACH_DECLARATION_OF_DIFFERS_IN_ITS_VALUE_WHERE_WAS_EXPECTED_BUT_WAS_GIVEN: &str = "Each declaration of '{0}.{1}' differs in its value, where '{2}' was expected but '{3}' was given.";
    pub const ONE_VALUE_OF_IS_THE_STRING_AND_THE_OTHER_IS_ASSUMED_TO_BE_AN_UNKNOWN_NUMERIC_VAL:
        &str = "One value of '{0}.{1}' is the string '{2}', and the other is assumed to be an unknown numeric value.";
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC: &str =
        "This member cannot have an 'override' modifier because its name is dynamic.";
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_NAME_IS:
        &str = "This member cannot have a JSDoc comment with an '@override' tag because its name is dynamic.";
    pub const THE_CURRENT_HOST_DOES_NOT_SUPPORT_THE_OPTION: &str =
        "The current host does not support the '{0}' option.";
    pub const CANNOT_FIND_THE_COMMON_SUBDIRECTORY_PATH_FOR_THE_INPUT_FILES: &str =
        "Cannot find the common subdirectory path for the input files.";
    pub const FILE_SPECIFICATION_CANNOT_END_IN_A_RECURSIVE_DIRECTORY_WILDCARD: &str =
        "File specification cannot end in a recursive directory wildcard ('**'): '{0}'.";
    pub const THE_COMMON_SOURCE_DIRECTORY_OF_IS_THE_ROOTDIR_SETTING_MUST_BE_EXPLICITLY_SET_TO:
        &str = "The common source directory of '{0}' is '{1}'. The 'rootDir' setting must be explicitly set to this or another path to adjust your output's file layout.";
    pub const CANNOT_READ_FILE: &str = "Cannot read file '{0}': {1}.";
    pub const UNKNOWN_COMPILER_OPTION: &str = "Unknown compiler option '{0}'.";
    pub const COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE: &str =
        "Compiler option '{0}' requires a value of type {1}.";
    pub const UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN: &str =
        "Unknown compiler option '{0}'. Did you mean '{1}'?";
    pub const COULD_NOT_WRITE_FILE: &str = "Could not write file '{0}': {1}.";
    pub const OPTION_PROJECT_CANNOT_BE_MIXED_WITH_SOURCE_FILES_ON_A_COMMAND_LINE: &str =
        "Option 'project' cannot be mixed with source files on a command line.";
    pub const OPTION_ISOLATEDMODULES_CAN_ONLY_BE_USED_WHEN_EITHER_OPTION_MODULE_IS_PROVIDED_OR:
        &str = "Option 'isolatedModules' can only be used when either option '--module' is provided or option 'target' is 'ES2015' or higher.";
    pub const OPTION_CAN_ONLY_BE_USED_WHEN_EITHER_OPTION_INLINESOURCEMAP_OR_OPTION_SOURCEMAP_I:
        &str = "Option '{0} can only be used when either option '--inlineSourceMap' or option '--sourceMap' is provided.";
    pub const OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION: &str =
        "Option '{0}' cannot be specified without specifying option '{1}'.";
    pub const OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION: &str =
        "Option '{0}' cannot be specified with option '{1}'.";
    pub const A_TSCONFIG_JSON_FILE_IS_ALREADY_DEFINED_AT: &str =
        "A 'tsconfig.json' file is already defined at: '{0}'.";
    pub const CANNOT_WRITE_FILE_BECAUSE_IT_WOULD_OVERWRITE_INPUT_FILE: &str =
        "Cannot write file '{0}' because it would overwrite input file.";
    pub const CANNOT_WRITE_FILE_BECAUSE_IT_WOULD_BE_OVERWRITTEN_BY_MULTIPLE_INPUT_FILES: &str =
        "Cannot write file '{0}' because it would be overwritten by multiple input files.";
    pub const CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY: &str =
        "Cannot find a tsconfig.json file at the specified directory: '{0}'.";
    pub const THE_SPECIFIED_PATH_DOES_NOT_EXIST: &str = "The specified path does not exist: '{0}'.";
    pub const INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER: &str =
        "Invalid value for '--reactNamespace'. '{0}' is not a valid identifier.";
    pub const PATTERN_CAN_HAVE_AT_MOST_ONE_CHARACTER: &str =
        "Pattern '{0}' can have at most one '*' character.";
    pub const SUBSTITUTION_IN_PATTERN_CAN_HAVE_AT_MOST_ONE_CHARACTER: &str =
        "Substitution '{0}' in pattern '{1}' can have at most one '*' character.";
    pub const SUBSTITUTIONS_FOR_PATTERN_SHOULD_BE_AN_ARRAY: &str =
        "Substitutions for pattern '{0}' should be an array.";
    pub const SUBSTITUTION_FOR_PATTERN_HAS_INCORRECT_TYPE_EXPECTED_STRING_GOT: &str =
        "Substitution '{0}' for pattern '{1}' has incorrect type, expected 'string', got '{2}'.";
    pub const FILE_SPECIFICATION_CANNOT_CONTAIN_A_PARENT_DIRECTORY_THAT_APPEARS_AFTER_A_RECURS:
        &str = "File specification cannot contain a parent directory ('..') that appears after a recursive directory wildcard ('**'): '{0}'.";
    pub const SUBSTITUTIONS_FOR_PATTERN_SHOULDNT_BE_AN_EMPTY_ARRAY: &str =
        "Substitutions for pattern '{0}' shouldn't be an empty array.";
    pub const INVALID_VALUE_FOR_JSXFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME: &str =
        "Invalid value for 'jsxFactory'. '{0}' is not a valid identifier or qualified-name.";
    pub const ADDING_A_TSCONFIG_JSON_FILE_WILL_HELP_ORGANIZE_PROJECTS_THAT_CONTAIN_BOTH_TYPESC:
        &str = "Adding a tsconfig.json file will help organize projects that contain both TypeScript and JavaScript files. Learn more at https://aka.ms/tsconfig.";
    pub const OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION: &str =
        "Option '{0}' cannot be specified without specifying option '{1}' or option '{2}'.";
    pub const OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULERESOLUTION_IS_SET_TO_CLA:
        &str = "Option '--resolveJsonModule' cannot be specified when 'moduleResolution' is set to 'classic'.";
    pub const OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULE_IS_SET_TO_NONE_SYSTEM_O:
        &str = "Option '--resolveJsonModule' cannot be specified when 'module' is set to 'none', 'system', or 'umd'.";
    pub const UNKNOWN_BUILD_OPTION: &str = "Unknown build option '{0}'.";
    pub const BUILD_OPTION_REQUIRES_A_VALUE_OF_TYPE: &str =
        "Build option '{0}' requires a value of type {1}.";
    pub const OPTION_INCREMENTAL_CAN_ONLY_BE_SPECIFIED_USING_TSCONFIG_EMITTING_TO_SINGLE_FILE:
        &str = "Option '--incremental' can only be specified using tsconfig, emitting to single file or when option '--tsBuildInfoFile' is specified.";
    pub const IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE:
        &str = "'{0}' is assignable to the constraint of type '{1}', but '{1}' could be instantiated with a different subtype of constraint '{2}'.";
    pub const AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES: &str =
        "'{0}' and '{1}' operations cannot be mixed without parentheses.";
    pub const UNKNOWN_BUILD_OPTION_DID_YOU_MEAN: &str =
        "Unknown build option '{0}'. Did you mean '{1}'?";
    pub const UNKNOWN_WATCH_OPTION: &str = "Unknown watch option '{0}'.";
    pub const UNKNOWN_WATCH_OPTION_DID_YOU_MEAN: &str =
        "Unknown watch option '{0}'. Did you mean '{1}'?";
    pub const WATCH_OPTION_REQUIRES_A_VALUE_OF_TYPE: &str =
        "Watch option '{0}' requires a value of type {1}.";
    pub const CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_CURRENT_DIRECTORY: &str =
        "Cannot find a tsconfig.json file at the current directory: {0}.";
    pub const COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO: &str =
        "'{0}' could be instantiated with an arbitrary type which could be unrelated to '{1}'.";
    pub const CANNOT_READ_FILE_2: &str = "Cannot read file '{0}'.";
    pub const A_TUPLE_MEMBER_CANNOT_BE_BOTH_OPTIONAL_AND_REST: &str =
        "A tuple member cannot be both optional and rest.";
    pub const A_LABELED_TUPLE_ELEMENT_IS_DECLARED_AS_OPTIONAL_WITH_A_QUESTION_MARK_AFTER_THE_N:
        &str = "A labeled tuple element is declared as optional with a question mark after the name and before the colon, rather than after the type.";
    pub const A_LABELED_TUPLE_ELEMENT_IS_DECLARED_AS_REST_WITH_A_BEFORE_THE_NAME_RATHER_THAN_B:
        &str = "A labeled tuple element is declared as rest with a '...' before the name, rather than before the type.";
    pub const THE_INFERRED_TYPE_OF_REFERENCES_A_TYPE_WITH_A_CYCLIC_STRUCTURE_WHICH_CANNOT_BE_T:
        &str = "The inferred type of '{0}' references a type with a cyclic structure which cannot be trivially serialized. A type annotation is necessary.";
    pub const OPTION_CANNOT_BE_SPECIFIED_WHEN_OPTION_JSX_IS: &str =
        "Option '{0}' cannot be specified when option 'jsx' is '{1}'.";
    pub const NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD:
        &str = "Non-relative paths are not allowed when 'baseUrl' is not set. Did you forget a leading './'?";
    pub const OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED: &str =
        "Option 'preserveConstEnums' cannot be disabled when '{0}' is enabled.";
    pub const THE_ROOT_VALUE_OF_A_FILE_MUST_BE_AN_OBJECT: &str =
        "The root value of a '{0}' file must be an object.";
    pub const COMPILER_OPTION_MAY_ONLY_BE_USED_WITH_BUILD: &str =
        "Compiler option '--{0}' may only be used with '--build'.";
    pub const COMPILER_OPTION_MAY_NOT_BE_USED_WITH_BUILD: &str =
        "Compiler option '--{0}' may not be used with '--build'.";
    pub const OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT:
        &str = "Option '{0}' can only be used when 'module' is set to 'preserve', 'commonjs', or 'es2015' or later.";
    pub const OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR:
        &str = "Option 'allowImportingTsExtensions' can only be used when one of 'noEmit', 'emitDeclarationOnly', or 'rewriteRelativeImportExtensions' is set.";
    pub const AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS:
        &str = "An import path can only end with a '{0}' extension when 'allowImportingTsExtensions' is enabled.";
    pub const OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL:
        &str = "Option '{0}' can only be used when 'moduleResolution' is set to 'node16', 'nodenext', or 'bundler'.";
    pub const OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT:
        &str = "Option '{0}' is deprecated and will stop functioning in TypeScript {1}. Specify compilerOption '\"ignoreDeprecations\": \"{2}\"' to silence this error.";
    pub const OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION: &str =
        "Option '{0}' has been removed. Please remove it from your configuration.";
    pub const INVALID_VALUE_FOR_IGNOREDEPRECATIONS: &str =
        "Invalid value for '--ignoreDeprecations'.";
    pub const OPTION_IS_REDUNDANT_AND_CANNOT_BE_SPECIFIED_WITH_OPTION: &str =
        "Option '{0}' is redundant and cannot be specified with option '{1}'.";
    pub const OPTION_VERBATIMMODULESYNTAX_CANNOT_BE_USED_WHEN_MODULE_IS_SET_TO_UMD_AMD_OR_SYST:
        &str = "Option 'verbatimModuleSyntax' cannot be used when 'module' is set to 'UMD', 'AMD', or 'System'.";
    pub const USE_INSTEAD: &str = "Use '{0}' instead.";
    pub const OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2:
        &str = "Option '{0}={1}' is deprecated and will stop functioning in TypeScript {2}. Specify compilerOption '\"ignoreDeprecations\": \"{3}\"' to silence this error.";
    pub const OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2: &str =
        "Option '{0}={1}' has been removed. Please remove it from your configuration.";
    pub const OPTION_MODULERESOLUTION_MUST_BE_SET_TO_OR_LEFT_UNSPECIFIED_WHEN_OPTION_MODULE_IS:
        &str = "Option 'moduleResolution' must be set to '{0}' (or left unspecified) when option 'module' is set to '{1}'.";
    pub const OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO: &str =
        "Option 'module' must be set to '{0}' when option 'moduleResolution' is set to '{1}'.";
    pub const VISIT_HTTPS_AKA_MS_TS6_FOR_MIGRATION_INFORMATION: &str =
        "Visit https://aka.ms/ts6 for migration information.";
    pub const TSCONFIG_JSON_IS_PRESENT_BUT_WILL_NOT_BE_LOADED_IF_FILES_ARE_SPECIFIED_ON_COMMAN:
        &str = "tsconfig.json is present but will not be loaded if files are specified on commandline. Use '--ignoreConfig' to skip this error.";
    pub const GENERATES_A_SOURCEMAP_FOR_EACH_CORRESPONDING_D_TS_FILE: &str =
        "Generates a sourcemap for each corresponding '.d.ts' file.";
    pub const CONCATENATE_AND_EMIT_OUTPUT_TO_SINGLE_FILE: &str =
        "Concatenate and emit output to single file.";
    pub const GENERATES_CORRESPONDING_D_TS_FILE: &str = "Generates corresponding '.d.ts' file.";
    pub const SPECIFY_THE_LOCATION_WHERE_DEBUGGER_SHOULD_LOCATE_TYPESCRIPT_FILES_INSTEAD_OF_SO:
        &str = "Specify the location where debugger should locate TypeScript files instead of source locations.";
    pub const WATCH_INPUT_FILES: &str = "Watch input files.";
    pub const REDIRECT_OUTPUT_STRUCTURE_TO_THE_DIRECTORY: &str =
        "Redirect output structure to the directory.";
    pub const DO_NOT_ERASE_CONST_ENUM_DECLARATIONS_IN_GENERATED_CODE: &str =
        "Do not erase const enum declarations in generated code.";
    pub const DO_NOT_EMIT_OUTPUTS_IF_ANY_ERRORS_WERE_REPORTED: &str =
        "Do not emit outputs if any errors were reported.";
    pub const DO_NOT_EMIT_COMMENTS_TO_OUTPUT: &str = "Do not emit comments to output.";
    pub const DO_NOT_EMIT_OUTPUTS: &str = "Do not emit outputs.";
    pub const ALLOW_DEFAULT_IMPORTS_FROM_MODULES_WITH_NO_DEFAULT_EXPORT_THIS_DOES_NOT_AFFECT_C:
        &str = "Allow default imports from modules with no default export. This does not affect code emit, just typechecking.";
    pub const SKIP_TYPE_CHECKING_OF_DECLARATION_FILES: &str =
        "Skip type checking of declaration files.";
    pub const DO_NOT_RESOLVE_THE_REAL_PATH_OF_SYMLINKS: &str =
        "Do not resolve the real path of symlinks.";
    pub const ONLY_EMIT_D_TS_DECLARATION_FILES: &str = "Only emit '.d.ts' declaration files.";
    pub const SPECIFY_ECMASCRIPT_TARGET_VERSION: &str = "Specify ECMAScript target version.";
    pub const SPECIFY_MODULE_CODE_GENERATION: &str = "Specify module code generation.";
    pub const PRINT_THIS_MESSAGE: &str = "Print this message.";
    pub const PRINT_THE_COMPILERS_VERSION: &str = "Print the compiler's version.";
    pub const COMPILE_THE_PROJECT_GIVEN_THE_PATH_TO_ITS_CONFIGURATION_FILE_OR_TO_A_FOLDER_WITH:
        &str = "Compile the project given the path to its configuration file, or to a folder with a 'tsconfig.json'.";
    pub const SYNTAX: &str = "Syntax: {0}";
    pub const OPTIONS: &str = "options";
    pub const FILE: &str = "file";
    pub const EXAMPLES: &str = "Examples: {0}";
    pub const OPTIONS_2: &str = "Options:";
    pub const VERSION: &str = "Version {0}";
    pub const INSERT_COMMAND_LINE_OPTIONS_AND_FILES_FROM_A_FILE: &str =
        "Insert command line options and files from a file.";
    pub const STARTING_COMPILATION_IN_WATCH_MODE: &str = "Starting compilation in watch mode...";
    pub const FILE_CHANGE_DETECTED_STARTING_INCREMENTAL_COMPILATION: &str =
        "File change detected. Starting incremental compilation...";
    pub const KIND: &str = "KIND";
    pub const FILE_2: &str = "FILE";
    pub const VERSION_2: &str = "VERSION";
    pub const LOCATION: &str = "LOCATION";
    pub const DIRECTORY: &str = "DIRECTORY";
    pub const STRATEGY: &str = "STRATEGY";
    pub const FILE_OR_DIRECTORY: &str = "FILE OR DIRECTORY";
    pub const ERRORS_FILES: &str = "Errors  Files";
    pub const GENERATES_CORRESPONDING_MAP_FILE: &str = "Generates corresponding '.map' file.";
    pub const COMPILER_OPTION_EXPECTS_AN_ARGUMENT: &str =
        "Compiler option '{0}' expects an argument.";
    pub const UNTERMINATED_QUOTED_STRING_IN_RESPONSE_FILE: &str =
        "Unterminated quoted string in response file '{0}'.";
    pub const ARGUMENT_FOR_OPTION_MUST_BE: &str = "Argument for '{0}' option must be: {1}.";
    pub const LOCALE_MUST_BE_OF_THE_FORM_LANGUAGE_OR_LANGUAGE_TERRITORY_FOR_EXAMPLE_OR: &str = "Locale must be of the form <language> or <language>-<territory>. For example '{0}' or '{1}'.";
    pub const UNABLE_TO_OPEN_FILE: &str = "Unable to open file '{0}'.";
    pub const CORRUPTED_LOCALE_FILE: &str = "Corrupted locale file {0}.";
    pub const RAISE_ERROR_ON_EXPRESSIONS_AND_DECLARATIONS_WITH_AN_IMPLIED_ANY_TYPE: &str =
        "Raise error on expressions and declarations with an implied 'any' type.";
    pub const FILE_NOT_FOUND: &str = "File '{0}' not found.";
    pub const FILE_HAS_AN_UNSUPPORTED_EXTENSION_THE_ONLY_SUPPORTED_EXTENSIONS_ARE: &str =
        "File '{0}' has an unsupported extension. The only supported extensions are {1}.";
    pub const SUPPRESS_NOIMPLICITANY_ERRORS_FOR_INDEXING_OBJECTS_LACKING_INDEX_SIGNATURES: &str =
        "Suppress noImplicitAny errors for indexing objects lacking index signatures.";
    pub const DO_NOT_EMIT_DECLARATIONS_FOR_CODE_THAT_HAS_AN_INTERNAL_ANNOTATION: &str =
        "Do not emit declarations for code that has an '@internal' annotation.";
    pub const SPECIFY_THE_ROOT_DIRECTORY_OF_INPUT_FILES_USE_TO_CONTROL_THE_OUTPUT_DIRECTORY_ST:
        &str = "Specify the root directory of input files. Use to control the output directory structure with --outDir.";
    pub const FILE_IS_NOT_UNDER_ROOTDIR_ROOTDIR_IS_EXPECTED_TO_CONTAIN_ALL_SOURCE_FILES: &str = "File '{0}' is not under 'rootDir' '{1}'. 'rootDir' is expected to contain all source files.";
    pub const SPECIFY_THE_END_OF_LINE_SEQUENCE_TO_BE_USED_WHEN_EMITTING_FILES_CRLF_DOS_OR_LF_U:
        &str = "Specify the end of line sequence to be used when emitting files: 'CRLF' (dos) or 'LF' (unix).";
    pub const NEWLINE: &str = "NEWLINE";
    pub const OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN:
        &str = "Option '{0}' can only be specified in 'tsconfig.json' file or set to 'null' on command line.";
    pub const ENABLES_EXPERIMENTAL_SUPPORT_FOR_ES7_DECORATORS: &str =
        "Enables experimental support for ES7 decorators.";
    pub const ENABLES_EXPERIMENTAL_SUPPORT_FOR_EMITTING_TYPE_METADATA_FOR_DECORATORS: &str =
        "Enables experimental support for emitting type metadata for decorators.";
    pub const INITIALIZES_A_TYPESCRIPT_PROJECT_AND_CREATES_A_TSCONFIG_JSON_FILE: &str =
        "Initializes a TypeScript project and creates a tsconfig.json file.";
    pub const SUCCESSFULLY_CREATED_A_TSCONFIG_JSON_FILE: &str =
        "Successfully created a tsconfig.json file.";
    pub const SUPPRESS_EXCESS_PROPERTY_CHECKS_FOR_OBJECT_LITERALS: &str =
        "Suppress excess property checks for object literals.";
    pub const STYLIZE_ERRORS_AND_MESSAGES_USING_COLOR_AND_CONTEXT_EXPERIMENTAL: &str =
        "Stylize errors and messages using color and context (experimental).";
    pub const DO_NOT_REPORT_ERRORS_ON_UNUSED_LABELS: &str =
        "Do not report errors on unused labels.";
    pub const REPORT_ERROR_WHEN_NOT_ALL_CODE_PATHS_IN_FUNCTION_RETURN_A_VALUE: &str =
        "Report error when not all code paths in function return a value.";
    pub const REPORT_ERRORS_FOR_FALLTHROUGH_CASES_IN_SWITCH_STATEMENT: &str =
        "Report errors for fallthrough cases in switch statement.";
    pub const DO_NOT_REPORT_ERRORS_ON_UNREACHABLE_CODE: &str =
        "Do not report errors on unreachable code.";
    pub const DISALLOW_INCONSISTENTLY_CASED_REFERENCES_TO_THE_SAME_FILE: &str =
        "Disallow inconsistently-cased references to the same file.";
    pub const SPECIFY_LIBRARY_FILES_TO_BE_INCLUDED_IN_THE_COMPILATION: &str =
        "Specify library files to be included in the compilation.";
    pub const SPECIFY_JSX_CODE_GENERATION: &str = "Specify JSX code generation.";
    pub const ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE: &str =
        "Only 'amd' and 'system' modules are supported alongside --{0}.";
    pub const BASE_DIRECTORY_TO_RESOLVE_NON_ABSOLUTE_MODULE_NAMES: &str =
        "Base directory to resolve non-absolute module names.";
    pub const DEPRECATED_USE_JSXFACTORY_INSTEAD_SPECIFY_THE_OBJECT_INVOKED_FOR_CREATEELEMENT_W:
        &str = "[Deprecated] Use '--jsxFactory' instead. Specify the object invoked for createElement when targeting 'react' JSX emit";
    pub const ENABLE_TRACING_OF_THE_NAME_RESOLUTION_PROCESS: &str =
        "Enable tracing of the name resolution process.";
    pub const RESOLVING_MODULE_FROM: &str = "======== Resolving module '{0}' from '{1}'. ========";
    pub const EXPLICITLY_SPECIFIED_MODULE_RESOLUTION_KIND: &str =
        "Explicitly specified module resolution kind: '{0}'.";
    pub const MODULE_RESOLUTION_KIND_IS_NOT_SPECIFIED_USING: &str =
        "Module resolution kind is not specified, using '{0}'.";
    pub const MODULE_NAME_WAS_SUCCESSFULLY_RESOLVED_TO: &str =
        "======== Module name '{0}' was successfully resolved to '{1}'. ========";
    pub const MODULE_NAME_WAS_NOT_RESOLVED: &str =
        "======== Module name '{0}' was not resolved. ========";
    pub const PATHS_OPTION_IS_SPECIFIED_LOOKING_FOR_A_PATTERN_TO_MATCH_MODULE_NAME: &str =
        "'paths' option is specified, looking for a pattern to match module name '{0}'.";
    pub const MODULE_NAME_MATCHED_PATTERN: &str = "Module name '{0}', matched pattern '{1}'.";
    pub const TRYING_SUBSTITUTION_CANDIDATE_MODULE_LOCATION: &str =
        "Trying substitution '{0}', candidate module location: '{1}'.";
    pub const RESOLVING_MODULE_NAME_RELATIVE_TO_BASE_URL: &str =
        "Resolving module name '{0}' relative to base url '{1}' - '{2}'.";
    pub const LOADING_MODULE_AS_FILE_FOLDER_CANDIDATE_MODULE_LOCATION_TARGET_FILE_TYPES: &str =
        "Loading module as file / folder, candidate module location '{0}', target file types: {1}.";
    pub const FILE_DOES_NOT_EXIST: &str = "File '{0}' does not exist.";
    pub const FILE_EXISTS_USE_IT_AS_A_NAME_RESOLUTION_RESULT: &str =
        "File '{0}' exists - use it as a name resolution result.";
    pub const LOADING_MODULE_FROM_NODE_MODULES_FOLDER_TARGET_FILE_TYPES: &str =
        "Loading module '{0}' from 'node_modules' folder, target file types: {1}.";
    pub const FOUND_PACKAGE_JSON_AT: &str = "Found 'package.json' at '{0}'.";
    pub const PACKAGE_JSON_DOES_NOT_HAVE_A_FIELD: &str =
        "'package.json' does not have a '{0}' field.";
    pub const PACKAGE_JSON_HAS_FIELD_THAT_REFERENCES: &str =
        "'package.json' has '{0}' field '{1}' that references '{2}'.";
    pub const ALLOW_JAVASCRIPT_FILES_TO_BE_COMPILED: &str =
        "Allow javascript files to be compiled.";
    pub const CHECKING_IF_IS_THE_LONGEST_MATCHING_PREFIX_FOR: &str =
        "Checking if '{0}' is the longest matching prefix for '{1}' - '{2}'.";
    pub const EXPECTED_TYPE_OF_FIELD_IN_PACKAGE_JSON_TO_BE_GOT: &str =
        "Expected type of '{0}' field in 'package.json' to be '{1}', got '{2}'.";
    pub const BASEURL_OPTION_IS_SET_TO_USING_THIS_VALUE_TO_RESOLVE_NON_RELATIVE_MODULE_NAME: &str = "'baseUrl' option is set to '{0}', using this value to resolve non-relative module name '{1}'.";
    pub const ROOTDIRS_OPTION_IS_SET_USING_IT_TO_RESOLVE_RELATIVE_MODULE_NAME: &str =
        "'rootDirs' option is set, using it to resolve relative module name '{0}'.";
    pub const LONGEST_MATCHING_PREFIX_FOR_IS: &str = "Longest matching prefix for '{0}' is '{1}'.";
    pub const LOADING_FROM_THE_ROOT_DIR_CANDIDATE_LOCATION: &str =
        "Loading '{0}' from the root dir '{1}', candidate location '{2}'.";
    pub const TRYING_OTHER_ENTRIES_IN_ROOTDIRS: &str = "Trying other entries in 'rootDirs'.";
    pub const MODULE_RESOLUTION_USING_ROOTDIRS_HAS_FAILED: &str =
        "Module resolution using 'rootDirs' has failed.";
    pub const DO_NOT_EMIT_USE_STRICT_DIRECTIVES_IN_MODULE_OUTPUT: &str =
        "Do not emit 'use strict' directives in module output.";
    pub const ENABLE_STRICT_NULL_CHECKS: &str = "Enable strict null checks.";
    pub const UNKNOWN_OPTION_EXCLUDES_DID_YOU_MEAN_EXCLUDE: &str =
        "Unknown option 'excludes'. Did you mean 'exclude'?";
    pub const RAISE_ERROR_ON_THIS_EXPRESSIONS_WITH_AN_IMPLIED_ANY_TYPE: &str =
        "Raise error on 'this' expressions with an implied 'any' type.";
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_ROOT_DIRECTORY: &str = "======== Resolving type reference directive '{0}', containing file '{1}', root directory '{2}'. ========";
    pub const TYPE_REFERENCE_DIRECTIVE_WAS_SUCCESSFULLY_RESOLVED_TO_PRIMARY: &str = "======== Type reference directive '{0}' was successfully resolved to '{1}', primary: {2}. ========";
    pub const TYPE_REFERENCE_DIRECTIVE_WAS_NOT_RESOLVED: &str =
        "======== Type reference directive '{0}' was not resolved. ========";
    pub const RESOLVING_WITH_PRIMARY_SEARCH_PATH: &str =
        "Resolving with primary search path '{0}'.";
    pub const ROOT_DIRECTORY_CANNOT_BE_DETERMINED_SKIPPING_PRIMARY_SEARCH_PATHS: &str =
        "Root directory cannot be determined, skipping primary search paths.";
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_ROOT_DIRECTORY_NOT_SET: &str = "======== Resolving type reference directive '{0}', containing file '{1}', root directory not set. ========";
    pub const TYPE_DECLARATION_FILES_TO_BE_INCLUDED_IN_COMPILATION: &str =
        "Type declaration files to be included in compilation.";
    pub const LOOKING_UP_IN_NODE_MODULES_FOLDER_INITIAL_LOCATION: &str =
        "Looking up in 'node_modules' folder, initial location '{0}'.";
    pub const CONTAINING_FILE_IS_NOT_SPECIFIED_AND_ROOT_DIRECTORY_CANNOT_BE_DETERMINED_SKIPPIN:
        &str = "Containing file is not specified and root directory cannot be determined, skipping lookup in 'node_modules' folder.";
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_NOT_SET_ROOT_DIRECTORY: &str = "======== Resolving type reference directive '{0}', containing file not set, root directory '{1}'. ========";
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_NOT_SET_ROOT_DIRECTORY_NOT_SE:
        &str = "======== Resolving type reference directive '{0}', containing file not set, root directory not set. ========";
    pub const RESOLVING_REAL_PATH_FOR_RESULT: &str = "Resolving real path for '{0}', result '{1}'.";
    pub const CANNOT_COMPILE_MODULES_USING_OPTION_UNLESS_THE_MODULE_FLAG_IS_AMD_OR_SYSTEM: &str = "Cannot compile modules using option '{0}' unless the '--module' flag is 'amd' or 'system'.";
    pub const FILE_NAME_HAS_A_EXTENSION_STRIPPING_IT: &str =
        "File name '{0}' has a '{1}' extension - stripping it.";
    pub const IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ: &str =
        "'{0}' is declared but its value is never read.";
    pub const REPORT_ERRORS_ON_UNUSED_LOCALS: &str = "Report errors on unused locals.";
    pub const REPORT_ERRORS_ON_UNUSED_PARAMETERS: &str = "Report errors on unused parameters.";
    pub const THE_MAXIMUM_DEPENDENCY_DEPTH_TO_SEARCH_UNDER_NODE_MODULES_AND_LOAD_JAVASCRIPT_FI:
        &str =
        "The maximum dependency depth to search under node_modules and load JavaScript files.";
    pub const CANNOT_IMPORT_TYPE_DECLARATION_FILES_CONSIDER_IMPORTING_INSTEAD_OF: &str =
        "Cannot import type declaration files. Consider importing '{0}' instead of '{1}'.";
    pub const PROPERTY_IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ: &str =
        "Property '{0}' is declared but its value is never read.";
    pub const IMPORT_EMIT_HELPERS_FROM_TSLIB: &str = "Import emit helpers from 'tslib'.";
    pub const AUTO_DISCOVERY_FOR_TYPINGS_IS_ENABLED_IN_PROJECT_RUNNING_EXTRA_RESOLUTION_PASS_F:
        &str = "Auto discovery for typings is enabled in project '{0}'. Running extra resolution pass for module '{1}' using cache location '{2}'.";
    pub const PARSE_IN_STRICT_MODE_AND_EMIT_USE_STRICT_FOR_EACH_SOURCE_FILE: &str =
        "Parse in strict mode and emit \"use strict\" for each source file.";
    pub const MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET: &str =
        "Module '{0}' was resolved to '{1}', but '--jsx' is not set.";
    pub const MODULE_WAS_RESOLVED_AS_LOCALLY_DECLARED_AMBIENT_MODULE_IN_FILE: &str =
        "Module '{0}' was resolved as locally declared ambient module in file '{1}'.";
    pub const SPECIFY_THE_JSX_FACTORY_FUNCTION_TO_USE_WHEN_TARGETING_REACT_JSX_EMIT_E_G_REACT:
        &str = "Specify the JSX factory function to use when targeting 'react' JSX emit, e.g. 'React.createElement' or 'h'.";
    pub const RESOLUTION_FOR_MODULE_WAS_FOUND_IN_CACHE_FROM_LOCATION: &str =
        "Resolution for module '{0}' was found in cache from location '{1}'.";
    pub const DIRECTORY_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT: &str =
        "Directory '{0}' does not exist, skipping all lookups in it.";
    pub const SHOW_DIAGNOSTIC_INFORMATION: &str = "Show diagnostic information.";
    pub const SHOW_VERBOSE_DIAGNOSTIC_INFORMATION: &str = "Show verbose diagnostic information.";
    pub const EMIT_A_SINGLE_FILE_WITH_SOURCE_MAPS_INSTEAD_OF_HAVING_A_SEPARATE_FILE: &str =
        "Emit a single file with source maps instead of having a separate file.";
    pub const EMIT_THE_SOURCE_ALONGSIDE_THE_SOURCEMAPS_WITHIN_A_SINGLE_FILE_REQUIRES_INLINESOU:
        &str = "Emit the source alongside the sourcemaps within a single file; requires '--inlineSourceMap' or '--sourceMap' to be set.";
    pub const TRANSPILE_EACH_FILE_AS_A_SEPARATE_MODULE_SIMILAR_TO_TS_TRANSPILEMODULE: &str =
        "Transpile each file as a separate module (similar to 'ts.transpileModule').";
    pub const PRINT_NAMES_OF_GENERATED_FILES_PART_OF_THE_COMPILATION: &str =
        "Print names of generated files part of the compilation.";
    pub const PRINT_NAMES_OF_FILES_PART_OF_THE_COMPILATION: &str =
        "Print names of files part of the compilation.";
    pub const THE_LOCALE_USED_WHEN_DISPLAYING_MESSAGES_TO_THE_USER_E_G_EN_US: &str =
        "The locale used when displaying messages to the user (e.g. 'en-us')";
    pub const DO_NOT_GENERATE_CUSTOM_HELPER_FUNCTIONS_LIKE_EXTENDS_IN_COMPILED_OUTPUT: &str =
        "Do not generate custom helper functions like '__extends' in compiled output.";
    pub const DO_NOT_INCLUDE_THE_DEFAULT_LIBRARY_FILE_LIB_D_TS: &str =
        "Do not include the default library file (lib.d.ts).";
    pub const DO_NOT_ADD_TRIPLE_SLASH_REFERENCES_OR_IMPORTED_MODULES_TO_THE_LIST_OF_COMPILED_F:
        &str =
        "Do not add triple-slash references or imported modules to the list of compiled files.";
    pub const DEPRECATED_USE_SKIPLIBCHECK_INSTEAD_SKIP_TYPE_CHECKING_OF_DEFAULT_LIBRARY_DECLAR:
        &str = "[Deprecated] Use '--skipLibCheck' instead. Skip type checking of default library declaration files.";
    pub const LIST_OF_FOLDERS_TO_INCLUDE_TYPE_DEFINITIONS_FROM: &str =
        "List of folders to include type definitions from.";
    pub const DISABLE_SIZE_LIMITATIONS_ON_JAVASCRIPT_PROJECTS: &str =
        "Disable size limitations on JavaScript projects.";
    pub const THE_CHARACTER_SET_OF_THE_INPUT_FILES: &str = "The character set of the input files.";
    pub const SKIPPING_MODULE_THAT_LOOKS_LIKE_AN_ABSOLUTE_URI_TARGET_FILE_TYPES: &str =
        "Skipping module '{0}' that looks like an absolute URI, target file types: {1}.";
    pub const DO_NOT_TRUNCATE_ERROR_MESSAGES: &str = "Do not truncate error messages.";
    pub const OUTPUT_DIRECTORY_FOR_GENERATED_DECLARATION_FILES: &str =
        "Output directory for generated declaration files.";
    pub const A_SERIES_OF_ENTRIES_WHICH_RE_MAP_IMPORTS_TO_LOOKUP_LOCATIONS_RELATIVE_TO_THE_BAS:
        &str =
        "A series of entries which re-map imports to lookup locations relative to the 'baseUrl'.";
    pub const LIST_OF_ROOT_FOLDERS_WHOSE_COMBINED_CONTENT_REPRESENTS_THE_STRUCTURE_OF_THE_PROJ:
        &str = "List of root folders whose combined content represents the structure of the project at runtime.";
    pub const SHOW_ALL_COMPILER_OPTIONS: &str = "Show all compiler options.";
    pub const DEPRECATED_USE_OUTFILE_INSTEAD_CONCATENATE_AND_EMIT_OUTPUT_TO_SINGLE_FILE: &str =
        "[Deprecated] Use '--outFile' instead. Concatenate and emit output to single file";
    pub const COMMAND_LINE_OPTIONS: &str = "Command-line Options";
    pub const PROVIDE_FULL_SUPPORT_FOR_ITERABLES_IN_FOR_OF_SPREAD_AND_DESTRUCTURING_WHEN_TARGE:
        &str = "Provide full support for iterables in 'for-of', spread, and destructuring when targeting 'ES5'.";
    pub const ENABLE_ALL_STRICT_TYPE_CHECKING_OPTIONS: &str =
        "Enable all strict type-checking options.";
    pub const SCOPED_PACKAGE_DETECTED_LOOKING_IN: &str =
        "Scoped package detected, looking in '{0}'";
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCESSFULLY_RESOLVED_TO:
        &str = "Reusing resolution of module '{0}' from '{1}' of old program, it was successfully resolved to '{2}'.";
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCESSFULLY_RESOLVED_TO_2:
        &str = "Reusing resolution of module '{0}' from '{1}' of old program, it was successfully resolved to '{2}' with Package ID '{3}'.";
    pub const ENABLE_STRICT_CHECKING_OF_FUNCTION_TYPES: &str =
        "Enable strict checking of function types.";
    pub const ENABLE_STRICT_CHECKING_OF_PROPERTY_INITIALIZATION_IN_CLASSES: &str =
        "Enable strict checking of property initialization in classes.";
    pub const NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE: &str =
        "Numeric separators are not allowed here.";
    pub const MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED: &str =
        "Multiple consecutive numeric separators are not permitted.";
    pub const WHETHER_TO_KEEP_OUTDATED_CONSOLE_OUTPUT_IN_WATCH_MODE_INSTEAD_OF_CLEARING_THE_SC:
        &str =
        "Whether to keep outdated console output in watch mode instead of clearing the screen.";
    pub const ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED: &str =
        "All imports in import declaration are unused.";
    pub const FOUND_1_ERROR_WATCHING_FOR_FILE_CHANGES: &str =
        "Found 1 error. Watching for file changes.";
    pub const FOUND_ERRORS_WATCHING_FOR_FILE_CHANGES: &str =
        "Found {0} errors. Watching for file changes.";
    pub const RESOLVE_KEYOF_TO_STRING_VALUED_PROPERTY_NAMES_ONLY_NO_NUMBERS_OR_SYMBOLS: &str =
        "Resolve 'keyof' to string valued property names only (no numbers or symbols).";
    pub const IS_DECLARED_BUT_NEVER_USED: &str = "'{0}' is declared but never used.";
    pub const INCLUDE_MODULES_IMPORTED_WITH_JSON_EXTENSION: &str =
        "Include modules imported with '.json' extension";
    pub const ALL_DESTRUCTURED_ELEMENTS_ARE_UNUSED: &str = "All destructured elements are unused.";
    pub const ALL_VARIABLES_ARE_UNUSED: &str = "All variables are unused.";
    pub const DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE: &str =
        "Definitions of the following identifiers conflict with those in another file: {0}";
    pub const CONFLICTS_ARE_IN_THIS_FILE: &str = "Conflicts are in this file.";
    pub const PROJECT_REFERENCES_MAY_NOT_FORM_A_CIRCULAR_GRAPH_CYCLE_DETECTED: &str =
        "Project references may not form a circular graph. Cycle detected: {0}";
    pub const WAS_ALSO_DECLARED_HERE: &str = "'{0}' was also declared here.";
    pub const AND_HERE: &str = "and here.";
    pub const ALL_TYPE_PARAMETERS_ARE_UNUSED: &str = "All type parameters are unused.";
    pub const PACKAGE_JSON_HAS_A_TYPESVERSIONS_FIELD_WITH_VERSION_SPECIFIC_PATH_MAPPINGS: &str =
        "'package.json' has a 'typesVersions' field with version-specific path mappings.";
    pub const PACKAGE_JSON_DOES_NOT_HAVE_A_TYPESVERSIONS_ENTRY_THAT_MATCHES_VERSION: &str =
        "'package.json' does not have a 'typesVersions' entry that matches version '{0}'.";
    pub const PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_THAT_MATCHES_COMPILER_VERSION_LOOKING_FOR:
        &str = "'package.json' has a 'typesVersions' entry '{0}' that matches compiler version '{1}', looking for a pattern to match module name '{2}'.";
    pub const PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_THAT_IS_NOT_A_VALID_SEMVER_RANGE: &str =
        "'package.json' has a 'typesVersions' entry '{0}' that is not a valid semver range.";
    pub const AN_ARGUMENT_FOR_WAS_NOT_PROVIDED: &str = "An argument for '{0}' was not provided.";
    pub const AN_ARGUMENT_MATCHING_THIS_BINDING_PATTERN_WAS_NOT_PROVIDED: &str =
        "An argument matching this binding pattern was not provided.";
    pub const DID_YOU_MEAN_TO_CALL_THIS_EXPRESSION: &str = "Did you mean to call this expression?";
    pub const DID_YOU_MEAN_TO_USE_NEW_WITH_THIS_EXPRESSION: &str =
        "Did you mean to use 'new' with this expression?";
    pub const ENABLE_STRICT_BIND_CALL_AND_APPLY_METHODS_ON_FUNCTIONS: &str =
        "Enable strict 'bind', 'call', and 'apply' methods on functions.";
    pub const USING_COMPILER_OPTIONS_OF_PROJECT_REFERENCE_REDIRECT: &str =
        "Using compiler options of project reference redirect '{0}'.";
    pub const FOUND_1_ERROR: &str = "Found 1 error.";
    pub const FOUND_ERRORS: &str = "Found {0} errors.";
    pub const MODULE_NAME_WAS_SUCCESSFULLY_RESOLVED_TO_WITH_PACKAGE_ID: &str = "======== Module name '{0}' was successfully resolved to '{1}' with Package ID '{2}'. ========";
    pub const TYPE_REFERENCE_DIRECTIVE_WAS_SUCCESSFULLY_RESOLVED_TO_WITH_PACKAGE_ID_PRIMARY: &str = "======== Type reference directive '{0}' was successfully resolved to '{1}' with Package ID '{2}', primary: {3}. ========";
    pub const PACKAGE_JSON_HAD_A_FALSY_FIELD: &str = "'package.json' had a falsy '{0}' field.";
    pub const DISABLE_USE_OF_SOURCE_FILES_INSTEAD_OF_DECLARATION_FILES_FROM_REFERENCED_PROJECT:
        &str = "Disable use of source files instead of declaration files from referenced projects.";
    pub const EMIT_CLASS_FIELDS_WITH_DEFINE_INSTEAD_OF_SET: &str =
        "Emit class fields with Define instead of Set.";
    pub const GENERATES_A_CPU_PROFILE: &str = "Generates a CPU profile.";
    pub const DISABLE_SOLUTION_SEARCHING_FOR_THIS_PROJECT: &str =
        "Disable solution searching for this project.";
    pub const SPECIFY_STRATEGY_FOR_WATCHING_FILE_FIXEDPOLLINGINTERVAL_DEFAULT_PRIORITYPOLLINGI:
        &str = "Specify strategy for watching file: 'FixedPollingInterval' (default), 'PriorityPollingInterval', 'DynamicPriorityPolling', 'FixedChunkSizePolling', 'UseFsEvents', 'UseFsEventsOnParentDirectory'.";
    pub const SPECIFY_STRATEGY_FOR_WATCHING_DIRECTORY_ON_PLATFORMS_THAT_DONT_SUPPORT_RECURSIVE:
        &str = "Specify strategy for watching directory on platforms that don't support recursive watching natively: 'UseFsEvents' (default), 'FixedPollingInterval', 'DynamicPriorityPolling', 'FixedChunkSizePolling'.";
    pub const SPECIFY_STRATEGY_FOR_CREATING_A_POLLING_WATCH_WHEN_IT_FAILS_TO_CREATE_USING_FILE:
        &str = "Specify strategy for creating a polling watch when it fails to create using file system events: 'FixedInterval' (default), 'PriorityInterval', 'DynamicPriority', 'FixedChunkSize'.";
    pub const TAG_EXPECTS_AT_LEAST_ARGUMENTS_BUT_THE_JSX_FACTORY_PROVIDES_AT_MOST: &str = "Tag '{0}' expects at least '{1}' arguments, but the JSX factory '{2}' provides at most '{3}'.";
    pub const OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_FALSE_OR_NULL_ON_CO:
        &str = "Option '{0}' can only be specified in 'tsconfig.json' file or set to 'false' or 'null' on command line.";
    pub const COULD_NOT_RESOLVE_THE_PATH_WITH_THE_EXTENSIONS: &str =
        "Could not resolve the path '{0}' with the extensions: {1}.";
    pub const DECLARATION_AUGMENTS_DECLARATION_IN_ANOTHER_FILE_THIS_CANNOT_BE_SERIALIZED: &str =
        "Declaration augments declaration in another file. This cannot be serialized.";
    pub const THIS_IS_THE_DECLARATION_BEING_AUGMENTED_CONSIDER_MOVING_THE_AUGMENTING_DECLARATI:
        &str = "This is the declaration being augmented. Consider moving the augmenting declaration into the same file.";
    pub const THIS_EXPRESSION_IS_NOT_CALLABLE_BECAUSE_IT_IS_A_GET_ACCESSOR_DID_YOU_MEAN_TO_USE:
        &str = "This expression is not callable because it is a 'get' accessor. Did you mean to use it without '()'?";
    pub const DISABLE_LOADING_REFERENCED_PROJECTS: &str = "Disable loading referenced projects.";
    pub const ARGUMENTS_FOR_THE_REST_PARAMETER_WERE_NOT_PROVIDED: &str =
        "Arguments for the rest parameter '{0}' were not provided.";
    pub const GENERATES_AN_EVENT_TRACE_AND_A_LIST_OF_TYPES: &str =
        "Generates an event trace and a list of types.";
    pub const SPECIFY_THE_MODULE_SPECIFIER_TO_BE_USED_TO_IMPORT_THE_JSX_AND_JSXS_FACTORY_FUNCT:
        &str = "Specify the module specifier to be used to import the 'jsx' and 'jsxs' factory functions from. eg, react";
    pub const FILE_EXISTS_ACCORDING_TO_EARLIER_CACHED_LOOKUPS: &str =
        "File '{0}' exists according to earlier cached lookups.";
    pub const FILE_DOES_NOT_EXIST_ACCORDING_TO_EARLIER_CACHED_LOOKUPS: &str =
        "File '{0}' does not exist according to earlier cached lookups.";
    pub const RESOLUTION_FOR_TYPE_REFERENCE_DIRECTIVE_WAS_FOUND_IN_CACHE_FROM_LOCATION: &str =
        "Resolution for type reference directive '{0}' was found in cache from location '{1}'.";
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE: &str =
        "======== Resolving type reference directive '{0}', containing file '{1}'. ========";
    pub const INTERPRET_OPTIONAL_PROPERTY_TYPES_AS_WRITTEN_RATHER_THAN_ADDING_UNDEFINED: &str =
        "Interpret optional property types as written, rather than adding 'undefined'.";
    pub const MODULES: &str = "Modules";
    pub const FILE_MANAGEMENT: &str = "File Management";
    pub const EMIT: &str = "Emit";
    pub const JAVASCRIPT_SUPPORT: &str = "JavaScript Support";
    pub const TYPE_CHECKING: &str = "Type Checking";
    pub const EDITOR_SUPPORT: &str = "Editor Support";
    pub const WATCH_AND_BUILD_MODES: &str = "Watch and Build Modes";
    pub const COMPILER_DIAGNOSTICS: &str = "Compiler Diagnostics";
    pub const INTEROP_CONSTRAINTS: &str = "Interop Constraints";
    pub const BACKWARDS_COMPATIBILITY: &str = "Backwards Compatibility";
    pub const LANGUAGE_AND_ENVIRONMENT: &str = "Language and Environment";
    pub const PROJECTS: &str = "Projects";
    pub const OUTPUT_FORMATTING: &str = "Output Formatting";
    pub const COMPLETENESS: &str = "Completeness";
    pub const SHOULD_BE_SET_INSIDE_THE_COMPILEROPTIONS_OBJECT_OF_THE_CONFIG_JSON_FILE: &str =
        "'{0}' should be set inside the 'compilerOptions' object of the config json file";
    pub const FOUND_1_ERROR_IN: &str = "Found 1 error in {0}";
    pub const FOUND_ERRORS_IN_THE_SAME_FILE_STARTING_AT: &str =
        "Found {0} errors in the same file, starting at: {1}";
    pub const FOUND_ERRORS_IN_FILES: &str = "Found {0} errors in {1} files.";
    pub const FILE_NAME_HAS_A_EXTENSION_LOOKING_UP_INSTEAD: &str =
        "File name '{0}' has a '{1}' extension - looking up '{2}' instead.";
    pub const MODULE_WAS_RESOLVED_TO_BUT_ALLOWARBITRARYEXTENSIONS_IS_NOT_SET: &str =
        "Module '{0}' was resolved to '{1}', but '--allowArbitraryExtensions' is not set.";
    pub const ENABLE_IMPORTING_FILES_WITH_ANY_EXTENSION_PROVIDED_A_DECLARATION_FILE_IS_PRESENT:
        &str = "Enable importing files with any extension, provided a declaration file is present.";
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_FOR_PROGRAM_THAT_SPECIFIES_CUSTOM_TYPEROOTS_S:
        &str = "Resolving type reference directive for program that specifies custom typeRoots, skipping lookup in 'node_modules' folder.";
    pub const OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE: &str =
        "Option '{0}' can only be specified on command line.";
    pub const DIRECTORY_HAS_NO_CONTAINING_PACKAGE_JSON_SCOPE_IMPORTS_WILL_NOT_RESOLVE: &str =
        "Directory '{0}' has no containing package.json scope. Imports will not resolve.";
    pub const IMPORT_SPECIFIER_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH: &str =
        "Import specifier '{0}' does not exist in package.json scope at path '{1}'.";
    pub const INVALID_IMPORT_SPECIFIER_HAS_NO_POSSIBLE_RESOLUTIONS: &str =
        "Invalid import specifier '{0}' has no possible resolutions.";
    pub const PACKAGE_JSON_SCOPE_HAS_NO_IMPORTS_DEFINED: &str =
        "package.json scope '{0}' has no imports defined.";
    pub const PACKAGE_JSON_SCOPE_EXPLICITLY_MAPS_SPECIFIER_TO_NULL: &str =
        "package.json scope '{0}' explicitly maps specifier '{1}' to null.";
    pub const PACKAGE_JSON_SCOPE_HAS_INVALID_TYPE_FOR_TARGET_OF_SPECIFIER: &str =
        "package.json scope '{0}' has invalid type for target of specifier '{1}'";
    pub const EXPORT_SPECIFIER_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH: &str =
        "Export specifier '{0}' does not exist in package.json scope at path '{1}'.";
    pub const RESOLUTION_OF_NON_RELATIVE_NAME_FAILED_TRYING_WITH_MODERN_NODE_RESOLUTION_FEATUR:
        &str = "Resolution of non-relative name failed; trying with modern Node resolution features disabled to see if npm library needs configuration update.";
    pub const THERE_ARE_TYPES_AT_BUT_THIS_RESULT_COULD_NOT_BE_RESOLVED_WHEN_RESPECTING_PACKAGE:
        &str = "There are types at '{0}', but this result could not be resolved when respecting package.json \"exports\". The '{1}' library may need to update its package.json or typings.";
    pub const RESOLUTION_OF_NON_RELATIVE_NAME_FAILED_TRYING_WITH_MODULERESOLUTION_BUNDLER_TO_S:
        &str = "Resolution of non-relative name failed; trying with '--moduleResolution bundler' to see if project may need configuration update.";
    pub const THERE_ARE_TYPES_AT_BUT_THIS_RESULT_COULD_NOT_BE_RESOLVED_UNDER_YOUR_CURRENT_MODU:
        &str = "There are types at '{0}', but this result could not be resolved under your current 'moduleResolution' setting. Consider updating to 'node16', 'nodenext', or 'bundler'.";
    pub const PACKAGE_JSON_HAS_A_PEERDEPENDENCIES_FIELD: &str =
        "'package.json' has a 'peerDependencies' field.";
    pub const FOUND_PEERDEPENDENCY_WITH_VERSION: &str =
        "Found peerDependency '{0}' with '{1}' version.";
    pub const FAILED_TO_FIND_PEERDEPENDENCY: &str = "Failed to find peerDependency '{0}'.";
    pub const FILE_LAYOUT: &str = "File Layout";
    pub const ENVIRONMENT_SETTINGS: &str = "Environment Settings";
    pub const SEE_ALSO_HTTPS_AKA_MS_TSCONFIG_MODULE: &str =
        "See also https://aka.ms/tsconfig/module";
    pub const FOR_NODEJS: &str = "For nodejs:";
    pub const AND_NPM_INSTALL_D_TYPES_NODE: &str = "and npm install -D @types/node";
    pub const OTHER_OUTPUTS: &str = "Other Outputs";
    pub const STRICTER_TYPECHECKING_OPTIONS: &str = "Stricter Typechecking Options";
    pub const STYLE_OPTIONS: &str = "Style Options";
    pub const RECOMMENDED_OPTIONS: &str = "Recommended Options";
    pub const ENABLE_PROJECT_COMPILATION: &str = "Enable project compilation";
    pub const COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT: &str =
        "Composite projects may not disable declaration emit.";
    pub const OUTPUT_FILE_HAS_NOT_BEEN_BUILT_FROM_SOURCE_FILE: &str =
        "Output file '{0}' has not been built from source file '{1}'.";
    pub const REFERENCED_PROJECT_MUST_HAVE_SETTING_COMPOSITE_TRUE: &str =
        "Referenced project '{0}' must have setting \"composite\": true.";
    pub const FILE_IS_NOT_LISTED_WITHIN_THE_FILE_LIST_OF_PROJECT_PROJECTS_MUST_LIST_ALL_FILES:
        &str = "File '{0}' is not listed within the file list of project '{1}'. Projects must list all files or use an 'include' pattern.";
    pub const REFERENCED_PROJECT_MAY_NOT_DISABLE_EMIT: &str =
        "Referenced project '{0}' may not disable emit.";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_OUTPUT_IS_OLDER_THAN_INPUT: &str =
        "Project '{0}' is out of date because output '{1}' is older than input '{2}'";
    pub const PROJECT_IS_UP_TO_DATE_BECAUSE_NEWEST_INPUT_IS_OLDER_THAN_OUTPUT: &str =
        "Project '{0}' is up to date because newest input '{1}' is older than output '{2}'";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_OUTPUT_FILE_DOES_NOT_EXIST: &str =
        "Project '{0}' is out of date because output file '{1}' does not exist";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_ITS_DEPENDENCY_IS_OUT_OF_DATE: &str =
        "Project '{0}' is out of date because its dependency '{1}' is out of date";
    pub const PROJECT_IS_UP_TO_DATE_WITH_D_TS_FILES_FROM_ITS_DEPENDENCIES: &str =
        "Project '{0}' is up to date with .d.ts files from its dependencies";
    pub const PROJECTS_IN_THIS_BUILD: &str = "Projects in this build: {0}";
    pub const A_NON_DRY_BUILD_WOULD_DELETE_THE_FOLLOWING_FILES: &str =
        "A non-dry build would delete the following files: {0}";
    pub const A_NON_DRY_BUILD_WOULD_BUILD_PROJECT: &str =
        "A non-dry build would build project '{0}'";
    pub const BUILDING_PROJECT: &str = "Building project '{0}'...";
    pub const UPDATING_OUTPUT_TIMESTAMPS_OF_PROJECT: &str =
        "Updating output timestamps of project '{0}'...";
    pub const PROJECT_IS_UP_TO_DATE: &str = "Project '{0}' is up to date";
    pub const SKIPPING_BUILD_OF_PROJECT_BECAUSE_ITS_DEPENDENCY_HAS_ERRORS: &str =
        "Skipping build of project '{0}' because its dependency '{1}' has errors";
    pub const PROJECT_CANT_BE_BUILT_BECAUSE_ITS_DEPENDENCY_HAS_ERRORS: &str =
        "Project '{0}' can't be built because its dependency '{1}' has errors";
    pub const BUILD_ONE_OR_MORE_PROJECTS_AND_THEIR_DEPENDENCIES_IF_OUT_OF_DATE: &str =
        "Build one or more projects and their dependencies, if out of date";
    pub const DELETE_THE_OUTPUTS_OF_ALL_PROJECTS: &str = "Delete the outputs of all projects.";
    pub const SHOW_WHAT_WOULD_BE_BUILT_OR_DELETED_IF_SPECIFIED_WITH_CLEAN: &str =
        "Show what would be built (or deleted, if specified with '--clean')";
    pub const OPTION_BUILD_MUST_BE_THE_FIRST_COMMAND_LINE_ARGUMENT: &str =
        "Option '--build' must be the first command line argument.";
    pub const OPTIONS_AND_CANNOT_BE_COMBINED: &str = "Options '{0}' and '{1}' cannot be combined.";
    pub const UPDATING_UNCHANGED_OUTPUT_TIMESTAMPS_OF_PROJECT: &str =
        "Updating unchanged output timestamps of project '{0}'...";
    pub const A_NON_DRY_BUILD_WOULD_UPDATE_TIMESTAMPS_FOR_OUTPUT_OF_PROJECT: &str =
        "A non-dry build would update timestamps for output of project '{0}'";
    pub const CANNOT_WRITE_FILE_BECAUSE_IT_WILL_OVERWRITE_TSBUILDINFO_FILE_GENERATED_BY_REFERE:
        &str = "Cannot write file '{0}' because it will overwrite '.tsbuildinfo' file generated by referenced project '{1}'";
    pub const COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION: &str =
        "Composite projects may not disable incremental compilation.";
    pub const SPECIFY_FILE_TO_STORE_INCREMENTAL_COMPILATION_INFORMATION: &str =
        "Specify file to store incremental compilation information";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_OUTPUT_FOR_IT_WAS_GENERATED_WITH_VERSION_THAT_DIF:
        &str = "Project '{0}' is out of date because output for it was generated with version '{1}' that differs with current version '{2}'";
    pub const SKIPPING_BUILD_OF_PROJECT_BECAUSE_ITS_DEPENDENCY_WAS_NOT_BUILT: &str =
        "Skipping build of project '{0}' because its dependency '{1}' was not built";
    pub const PROJECT_CANT_BE_BUILT_BECAUSE_ITS_DEPENDENCY_WAS_NOT_BUILT: &str =
        "Project '{0}' can't be built because its dependency '{1}' was not built";
    pub const HAVE_RECOMPILES_IN_INCREMENTAL_AND_WATCH_ASSUME_THAT_CHANGES_WITHIN_A_FILE_WILL:
        &str = "Have recompiles in '--incremental' and '--watch' assume that changes within a file will only affect files directly depending on it.";
    pub const IS_DEPRECATED: &str = "'{0}' is deprecated.";
    pub const PERFORMANCE_TIMINGS_FOR_DIAGNOSTICS_OR_EXTENDEDDIAGNOSTICS_ARE_NOT_AVAILABLE_IN:
        &str = "Performance timings for '--diagnostics' or '--extendedDiagnostics' are not available in this session. A native implementation of the Web Performance API could not be found.";
    pub const THE_SIGNATURE_OF_IS_DEPRECATED: &str = "The signature '{0}' of '{1}' is deprecated.";
    pub const PROJECT_IS_BEING_FORCIBLY_REBUILT: &str = "Project '{0}' is being forcibly rebuilt";
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_OF_OLD_PROGRAM_IT_WAS_NOT_RESOLVED: &str =
        "Reusing resolution of module '{0}' from '{1}' of old program, it was not resolved.";
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCES:
        &str = "Reusing resolution of type reference directive '{0}' from '{1}' of old program, it was successfully resolved to '{2}'.";
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCES_2:
        &str = "Reusing resolution of type reference directive '{0}' from '{1}' of old program, it was successfully resolved to '{2}' with Package ID '{3}'.";
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_OF_OLD_PROGRAM_IT_WAS_NOT_RE:
        &str = "Reusing resolution of type reference directive '{0}' from '{1}' of old program, it was not resolved.";
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_FOUND_IN_CACHE_FROM_LOCATION_IT_WAS_SUCCESSFUL:
        &str = "Reusing resolution of module '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}'.";
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_FOUND_IN_CACHE_FROM_LOCATION_IT_WAS_SUCCESSFUL_2:
        &str = "Reusing resolution of module '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}' with Package ID '{4}'.";
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_FOUND_IN_CACHE_FROM_LOCATION_IT_WAS_NOT_RESOLV:
        &str = "Reusing resolution of module '{0}' from '{1}' found in cache from location '{2}', it was not resolved.";
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_FOUND_IN_CACHE_FROM_LOCATION:
        &str = "Reusing resolution of type reference directive '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}'.";
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_FOUND_IN_CACHE_FROM_LOCATION_2:
        &str = "Reusing resolution of type reference directive '{0}' from '{1}' found in cache from location '{2}', it was successfully resolved to '{3}' with Package ID '{4}'.";
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_FOUND_IN_CACHE_FROM_LOCATION_3:
        &str = "Reusing resolution of type reference directive '{0}' from '{1}' found in cache from location '{2}', it was not resolved.";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THAT_SOME_OF_THE_CHANGES:
        &str = "Project '{0}' is out of date because buildinfo file '{1}' indicates that some of the changes were not emitted";
    pub const PROJECT_IS_UP_TO_DATE_BUT_NEEDS_TO_UPDATE_TIMESTAMPS_OF_OUTPUT_FILES_THAT_ARE_OL:
        &str = "Project '{0}' is up to date but needs to update timestamps of output files that are older than input files";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_THERE_WAS_ERROR_READING_FILE: &str =
        "Project '{0}' is out of date because there was error reading file '{1}'";
    pub const RESOLVING_IN_MODE_WITH_CONDITIONS: &str =
        "Resolving in {0} mode with conditions {1}.";
    pub const MATCHED_CONDITION: &str = "Matched '{0}' condition '{1}'.";
    pub const USING_SUBPATH_WITH_TARGET: &str = "Using '{0}' subpath '{1}' with target '{2}'.";
    pub const SAW_NON_MATCHING_CONDITION: &str = "Saw non-matching condition '{0}'.";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THERE_IS_CHANGE_IN_COMPI:
        &str = "Project '{0}' is out of date because buildinfo file '{1}' indicates there is change in compilerOptions";
    pub const ALLOW_IMPORTS_TO_INCLUDE_TYPESCRIPT_FILE_EXTENSIONS_REQUIRES_MODULERESOLUTION_BU:
        &str = "Allow imports to include TypeScript file extensions. Requires '--moduleResolution bundler' and either '--noEmit' or '--emitDeclarationOnly' to be set.";
    pub const USE_THE_PACKAGE_JSON_EXPORTS_FIELD_WHEN_RESOLVING_PACKAGE_IMPORTS: &str =
        "Use the package.json 'exports' field when resolving package imports.";
    pub const USE_THE_PACKAGE_JSON_IMPORTS_FIELD_WHEN_RESOLVING_IMPORTS: &str =
        "Use the package.json 'imports' field when resolving imports.";
    pub const CONDITIONS_TO_SET_IN_ADDITION_TO_THE_RESOLVER_SPECIFIC_DEFAULTS_WHEN_RESOLVING_I:
        &str =
        "Conditions to set in addition to the resolver-specific defaults when resolving imports.";
    pub const TRUE_WHEN_MODULERESOLUTION_IS_NODE16_NODENEXT_OR_BUNDLER_OTHERWISE_FALSE: &str =
        "`true` when 'moduleResolution' is 'node16', 'nodenext', or 'bundler'; otherwise `false`.";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THAT_FILE_WAS_ROOT_FILE:
        &str = "Project '{0}' is out of date because buildinfo file '{1}' indicates that file '{2}' was root file of compilation but not any more.";
    pub const ENTERING_CONDITIONAL_EXPORTS: &str = "Entering conditional exports.";
    pub const RESOLVED_UNDER_CONDITION: &str = "Resolved under condition '{0}'.";
    pub const FAILED_TO_RESOLVE_UNDER_CONDITION: &str = "Failed to resolve under condition '{0}'.";
    pub const EXITING_CONDITIONAL_EXPORTS: &str = "Exiting conditional exports.";
    pub const SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_PREFERRED_EXTENSIONS: &str =
        "Searching all ancestor node_modules directories for preferred extensions: {0}.";
    pub const SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_FALLBACK_EXTENSIONS: &str =
        "Searching all ancestor node_modules directories for fallback extensions: {0}.";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THAT_PROGRAM_NEEDS_TO_RE:
        &str = "Project '{0}' is out of date because buildinfo file '{1}' indicates that program needs to report errors.";
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE: &str = "Project '{0}' is out of date because {1}.";
    pub const REWRITE_TS_TSX_MTS_AND_CTS_FILE_EXTENSIONS_IN_RELATIVE_IMPORT_PATHS_TO_THEIR_JAV:
        &str = "Rewrite '.ts', '.tsx', '.mts', and '.cts' file extensions in relative import paths to their JavaScript equivalent in output files.";
    pub const THE_EXPECTED_TYPE_COMES_FROM_PROPERTY_WHICH_IS_DECLARED_HERE_ON_TYPE: &str =
        "The expected type comes from property '{0}' which is declared here on type '{1}'";
    pub const THE_EXPECTED_TYPE_COMES_FROM_THIS_INDEX_SIGNATURE: &str =
        "The expected type comes from this index signature.";
    pub const THE_EXPECTED_TYPE_COMES_FROM_THE_RETURN_TYPE_OF_THIS_SIGNATURE: &str =
        "The expected type comes from the return type of this signature.";
    pub const PRINT_NAMES_OF_FILES_THAT_ARE_PART_OF_THE_COMPILATION_AND_THEN_STOP_PROCESSING: &str =
        "Print names of files that are part of the compilation and then stop processing.";
    pub const FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION: &str =
        "File '{0}' is a JavaScript file. Did you mean to enable the 'allowJs' option?";
    pub const PRINT_NAMES_OF_FILES_AND_THE_REASON_THEY_ARE_PART_OF_THE_COMPILATION: &str =
        "Print names of files and the reason they are part of the compilation.";
    pub const CONSIDER_ADDING_A_DECLARE_MODIFIER_TO_THIS_CLASS: &str =
        "Consider adding a 'declare' modifier to this class.";
    pub const ALLOW_JAVASCRIPT_FILES_TO_BE_A_PART_OF_YOUR_PROGRAM_USE_THE_CHECKJS_OPTION_TO_GE:
        &str = "Allow JavaScript files to be a part of your program. Use the 'checkJs' option to get errors from these files.";
    pub const ALLOW_IMPORT_X_FROM_Y_WHEN_A_MODULE_DOESNT_HAVE_A_DEFAULT_EXPORT: &str =
        "Allow 'import x from y' when a module doesn't have a default export.";
    pub const ALLOW_ACCESSING_UMD_GLOBALS_FROM_MODULES: &str =
        "Allow accessing UMD globals from modules.";
    pub const DISABLE_ERROR_REPORTING_FOR_UNREACHABLE_CODE: &str =
        "Disable error reporting for unreachable code.";
    pub const DISABLE_ERROR_REPORTING_FOR_UNUSED_LABELS: &str =
        "Disable error reporting for unused labels.";
    pub const ENSURE_USE_STRICT_IS_ALWAYS_EMITTED: &str = "Ensure 'use strict' is always emitted.";
    pub const HAVE_RECOMPILES_IN_PROJECTS_THAT_USE_INCREMENTAL_AND_WATCH_MODE_ASSUME_THAT_CHAN:
        &str = "Have recompiles in projects that use 'incremental' and 'watch' mode assume that changes within a file will only affect files directly depending on it.";
    pub const SPECIFY_THE_BASE_DIRECTORY_TO_RESOLVE_NON_RELATIVE_MODULE_NAMES: &str =
        "Specify the base directory to resolve non-relative module names.";
    pub const NO_LONGER_SUPPORTED_IN_EARLY_VERSIONS_MANUALLY_SET_THE_TEXT_ENCODING_FOR_READING:
        &str =
        "No longer supported. In early versions, manually set the text encoding for reading files.";
    pub const ENABLE_ERROR_REPORTING_IN_TYPE_CHECKED_JAVASCRIPT_FILES: &str =
        "Enable error reporting in type-checked JavaScript files.";
    pub const ENABLE_CONSTRAINTS_THAT_ALLOW_A_TYPESCRIPT_PROJECT_TO_BE_USED_WITH_PROJECT_REFER:
        &str =
        "Enable constraints that allow a TypeScript project to be used with project references.";
    pub const GENERATE_D_TS_FILES_FROM_TYPESCRIPT_AND_JAVASCRIPT_FILES_IN_YOUR_PROJECT: &str =
        "Generate .d.ts files from TypeScript and JavaScript files in your project.";
    pub const SPECIFY_THE_OUTPUT_DIRECTORY_FOR_GENERATED_DECLARATION_FILES: &str =
        "Specify the output directory for generated declaration files.";
    pub const CREATE_SOURCEMAPS_FOR_D_TS_FILES: &str = "Create sourcemaps for d.ts files.";
    pub const OUTPUT_COMPILER_PERFORMANCE_INFORMATION_AFTER_BUILDING: &str =
        "Output compiler performance information after building.";
    pub const DISABLES_INFERENCE_FOR_TYPE_ACQUISITION_BY_LOOKING_AT_FILENAMES_IN_A_PROJECT: &str =
        "Disables inference for type acquisition by looking at filenames in a project.";
    pub const REDUCE_THE_NUMBER_OF_PROJECTS_LOADED_AUTOMATICALLY_BY_TYPESCRIPT: &str =
        "Reduce the number of projects loaded automatically by TypeScript.";
    pub const REMOVE_THE_20MB_CAP_ON_TOTAL_SOURCE_CODE_SIZE_FOR_JAVASCRIPT_FILES_IN_THE_TYPESC:
        &str = "Remove the 20mb cap on total source code size for JavaScript files in the TypeScript language server.";
    pub const OPT_A_PROJECT_OUT_OF_MULTI_PROJECT_REFERENCE_CHECKING_WHEN_EDITING: &str =
        "Opt a project out of multi-project reference checking when editing.";
    pub const DISABLE_PREFERRING_SOURCE_FILES_INSTEAD_OF_DECLARATION_FILES_WHEN_REFERENCING_CO:
        &str = "Disable preferring source files instead of declaration files when referencing composite projects.";
    pub const EMIT_MORE_COMPLIANT_BUT_VERBOSE_AND_LESS_PERFORMANT_JAVASCRIPT_FOR_ITERATION: &str =
        "Emit more compliant, but verbose and less performant JavaScript for iteration.";
    pub const EMIT_A_UTF_8_BYTE_ORDER_MARK_BOM_IN_THE_BEGINNING_OF_OUTPUT_FILES: &str =
        "Emit a UTF-8 Byte Order Mark (BOM) in the beginning of output files.";
    pub const ONLY_OUTPUT_D_TS_FILES_AND_NOT_JAVASCRIPT_FILES: &str =
        "Only output d.ts files and not JavaScript files.";
    pub const EMIT_DESIGN_TYPE_METADATA_FOR_DECORATED_DECLARATIONS_IN_SOURCE_FILES: &str =
        "Emit design-type metadata for decorated declarations in source files.";
    pub const DISABLE_THE_TYPE_ACQUISITION_FOR_JAVASCRIPT_PROJECTS: &str =
        "Disable the type acquisition for JavaScript projects";
    pub const EMIT_ADDITIONAL_JAVASCRIPT_TO_EASE_SUPPORT_FOR_IMPORTING_COMMONJS_MODULES_THIS_E:
        &str = "Emit additional JavaScript to ease support for importing CommonJS modules. This enables 'allowSyntheticDefaultImports' for type compatibility.";
    pub const FILTERS_RESULTS_FROM_THE_INCLUDE_OPTION: &str =
        "Filters results from the `include` option.";
    pub const REMOVE_A_LIST_OF_DIRECTORIES_FROM_THE_WATCH_PROCESS: &str =
        "Remove a list of directories from the watch process.";
    pub const REMOVE_A_LIST_OF_FILES_FROM_THE_WATCH_MODES_PROCESSING: &str =
        "Remove a list of files from the watch mode's processing.";
    pub const ENABLE_EXPERIMENTAL_SUPPORT_FOR_LEGACY_EXPERIMENTAL_DECORATORS: &str =
        "Enable experimental support for legacy experimental decorators.";
    pub const PRINT_FILES_READ_DURING_THE_COMPILATION_INCLUDING_WHY_IT_WAS_INCLUDED: &str =
        "Print files read during the compilation including why it was included.";
    pub const OUTPUT_MORE_DETAILED_COMPILER_PERFORMANCE_INFORMATION_AFTER_BUILDING: &str =
        "Output more detailed compiler performance information after building.";
    pub const SPECIFY_ONE_OR_MORE_PATH_OR_NODE_MODULE_REFERENCES_TO_BASE_CONFIGURATION_FILES_F:
        &str = "Specify one or more path or node module references to base configuration files from which settings are inherited.";
    pub const SPECIFY_WHAT_APPROACH_THE_WATCHER_SHOULD_USE_IF_THE_SYSTEM_RUNS_OUT_OF_NATIVE_FI:
        &str = "Specify what approach the watcher should use if the system runs out of native file watchers.";
    pub const INCLUDE_A_LIST_OF_FILES_THIS_DOES_NOT_SUPPORT_GLOB_PATTERNS_AS_OPPOSED_TO_INCLUD:
        &str =
        "Include a list of files. This does not support glob patterns, as opposed to `include`.";
    pub const BUILD_ALL_PROJECTS_INCLUDING_THOSE_THAT_APPEAR_TO_BE_UP_TO_DATE: &str =
        "Build all projects, including those that appear to be up to date.";
    pub const ENSURE_THAT_CASING_IS_CORRECT_IN_IMPORTS: &str =
        "Ensure that casing is correct in imports.";
    pub const EMIT_A_V8_CPU_PROFILE_OF_THE_COMPILER_RUN_FOR_DEBUGGING: &str =
        "Emit a v8 CPU profile of the compiler run for debugging.";
    pub const ALLOW_IMPORTING_HELPER_FUNCTIONS_FROM_TSLIB_ONCE_PER_PROJECT_INSTEAD_OF_INCLUDIN:
        &str = "Allow importing helper functions from tslib once per project, instead of including them per-file.";
    pub const SKIP_BUILDING_DOWNSTREAM_PROJECTS_ON_ERROR_IN_UPSTREAM_PROJECT: &str =
        "Skip building downstream projects on error in upstream project.";
    pub const SPECIFY_A_LIST_OF_GLOB_PATTERNS_THAT_MATCH_FILES_TO_BE_INCLUDED_IN_COMPILATION: &str =
        "Specify a list of glob patterns that match files to be included in compilation.";
    pub const SAVE_TSBUILDINFO_FILES_TO_ALLOW_FOR_INCREMENTAL_COMPILATION_OF_PROJECTS: &str =
        "Save .tsbuildinfo files to allow for incremental compilation of projects.";
    pub const INCLUDE_SOURCEMAP_FILES_INSIDE_THE_EMITTED_JAVASCRIPT: &str =
        "Include sourcemap files inside the emitted JavaScript.";
    pub const INCLUDE_SOURCE_CODE_IN_THE_SOURCEMAPS_INSIDE_THE_EMITTED_JAVASCRIPT: &str =
        "Include source code in the sourcemaps inside the emitted JavaScript.";
    pub const ENSURE_THAT_EACH_FILE_CAN_BE_SAFELY_TRANSPILED_WITHOUT_RELYING_ON_OTHER_IMPORTS:
        &str = "Ensure that each file can be safely transpiled without relying on other imports.";
    pub const SPECIFY_WHAT_JSX_CODE_IS_GENERATED: &str = "Specify what JSX code is generated.";
    pub const SPECIFY_THE_JSX_FACTORY_FUNCTION_USED_WHEN_TARGETING_REACT_JSX_EMIT_E_G_REACT_CR:
        &str = "Specify the JSX factory function used when targeting React JSX emit, e.g. 'React.createElement' or 'h'.";
    pub const SPECIFY_THE_JSX_FRAGMENT_REFERENCE_USED_FOR_FRAGMENTS_WHEN_TARGETING_REACT_JSX_E:
        &str = "Specify the JSX Fragment reference used for fragments when targeting React JSX emit e.g. 'React.Fragment' or 'Fragment'.";
    pub const SPECIFY_MODULE_SPECIFIER_USED_TO_IMPORT_THE_JSX_FACTORY_FUNCTIONS_WHEN_USING_JSX:
        &str = "Specify module specifier used to import the JSX factory functions when using 'jsx: react-jsx*'.";
    pub const MAKE_KEYOF_ONLY_RETURN_STRINGS_INSTEAD_OF_STRING_NUMBERS_OR_SYMBOLS_LEGACY_OPTIO:
        &str =
        "Make keyof only return strings instead of string, numbers or symbols. Legacy option.";
    pub const SPECIFY_A_SET_OF_BUNDLED_LIBRARY_DECLARATION_FILES_THAT_DESCRIBE_THE_TARGET_RUNT:
        &str = "Specify a set of bundled library declaration files that describe the target runtime environment.";
    pub const PRINT_THE_NAMES_OF_EMITTED_FILES_AFTER_A_COMPILATION: &str =
        "Print the names of emitted files after a compilation.";
    pub const PRINT_ALL_OF_THE_FILES_READ_DURING_THE_COMPILATION: &str =
        "Print all of the files read during the compilation.";
    pub const SET_THE_LANGUAGE_OF_THE_MESSAGING_FROM_TYPESCRIPT_THIS_DOES_NOT_AFFECT_EMIT: &str =
        "Set the language of the messaging from TypeScript. This does not affect emit.";
    pub const SPECIFY_THE_LOCATION_WHERE_DEBUGGER_SHOULD_LOCATE_MAP_FILES_INSTEAD_OF_GENERATED:
        &str = "Specify the location where debugger should locate map files instead of generated locations.";
    pub const SPECIFY_THE_MAXIMUM_FOLDER_DEPTH_USED_FOR_CHECKING_JAVASCRIPT_FILES_FROM_NODE_MO:
        &str = "Specify the maximum folder depth used for checking JavaScript files from 'node_modules'. Only applicable with 'allowJs'.";
    pub const SPECIFY_WHAT_MODULE_CODE_IS_GENERATED: &str =
        "Specify what module code is generated.";
    pub const SPECIFY_HOW_TYPESCRIPT_LOOKS_UP_A_FILE_FROM_A_GIVEN_MODULE_SPECIFIER: &str =
        "Specify how TypeScript looks up a file from a given module specifier.";
    pub const SET_THE_NEWLINE_CHARACTER_FOR_EMITTING_FILES: &str =
        "Set the newline character for emitting files.";
    pub const DISABLE_EMITTING_FILES_FROM_A_COMPILATION: &str =
        "Disable emitting files from a compilation.";
    pub const DISABLE_GENERATING_CUSTOM_HELPER_FUNCTIONS_LIKE_EXTENDS_IN_COMPILED_OUTPUT: &str =
        "Disable generating custom helper functions like '__extends' in compiled output.";
    pub const DISABLE_EMITTING_FILES_IF_ANY_TYPE_CHECKING_ERRORS_ARE_REPORTED: &str =
        "Disable emitting files if any type checking errors are reported.";
    pub const DISABLE_TRUNCATING_TYPES_IN_ERROR_MESSAGES: &str =
        "Disable truncating types in error messages.";
    pub const ENABLE_ERROR_REPORTING_FOR_FALLTHROUGH_CASES_IN_SWITCH_STATEMENTS: &str =
        "Enable error reporting for fallthrough cases in switch statements.";
    pub const ENABLE_ERROR_REPORTING_FOR_EXPRESSIONS_AND_DECLARATIONS_WITH_AN_IMPLIED_ANY_TYPE:
        &str =
        "Enable error reporting for expressions and declarations with an implied 'any' type.";
    pub const ENSURE_OVERRIDING_MEMBERS_IN_DERIVED_CLASSES_ARE_MARKED_WITH_AN_OVERRIDE_MODIFIE:
        &str = "Ensure overriding members in derived classes are marked with an override modifier.";
    pub const ENABLE_ERROR_REPORTING_FOR_CODEPATHS_THAT_DO_NOT_EXPLICITLY_RETURN_IN_A_FUNCTION:
        &str = "Enable error reporting for codepaths that do not explicitly return in a function.";
    pub const ENABLE_ERROR_REPORTING_WHEN_THIS_IS_GIVEN_THE_TYPE_ANY: &str =
        "Enable error reporting when 'this' is given the type 'any'.";
    pub const DISABLE_ADDING_USE_STRICT_DIRECTIVES_IN_EMITTED_JAVASCRIPT_FILES: &str =
        "Disable adding 'use strict' directives in emitted JavaScript files.";
    pub const DISABLE_INCLUDING_ANY_LIBRARY_FILES_INCLUDING_THE_DEFAULT_LIB_D_TS: &str =
        "Disable including any library files, including the default lib.d.ts.";
    pub const ENFORCES_USING_INDEXED_ACCESSORS_FOR_KEYS_DECLARED_USING_AN_INDEXED_TYPE: &str =
        "Enforces using indexed accessors for keys declared using an indexed type.";
    pub const DISALLOW_IMPORTS_REQUIRES_OR_REFERENCE_S_FROM_EXPANDING_THE_NUMBER_OF_FILES_TYPE:
        &str = "Disallow 'import's, 'require's or '<reference>'s from expanding the number of files TypeScript should add to a project.";
    pub const DISABLE_STRICT_CHECKING_OF_GENERIC_SIGNATURES_IN_FUNCTION_TYPES: &str =
        "Disable strict checking of generic signatures in function types.";
    pub const ADD_UNDEFINED_TO_A_TYPE_WHEN_ACCESSED_USING_AN_INDEX: &str =
        "Add 'undefined' to a type when accessed using an index.";
    pub const ENABLE_ERROR_REPORTING_WHEN_LOCAL_VARIABLES_ARENT_READ: &str =
        "Enable error reporting when local variables aren't read.";
    pub const RAISE_AN_ERROR_WHEN_A_FUNCTION_PARAMETER_ISNT_READ: &str =
        "Raise an error when a function parameter isn't read.";
    pub const DEPRECATED_SETTING_USE_OUTFILE_INSTEAD: &str =
        "Deprecated setting. Use 'outFile' instead.";
    pub const SPECIFY_AN_OUTPUT_FOLDER_FOR_ALL_EMITTED_FILES: &str =
        "Specify an output folder for all emitted files.";
    pub const SPECIFY_A_FILE_THAT_BUNDLES_ALL_OUTPUTS_INTO_ONE_JAVASCRIPT_FILE_IF_DECLARATION:
        &str = "Specify a file that bundles all outputs into one JavaScript file. If 'declaration' is true, also designates a file that bundles all .d.ts output.";
    pub const SPECIFY_A_SET_OF_ENTRIES_THAT_RE_MAP_IMPORTS_TO_ADDITIONAL_LOOKUP_LOCATIONS: &str =
        "Specify a set of entries that re-map imports to additional lookup locations.";
    pub const SPECIFY_A_LIST_OF_LANGUAGE_SERVICE_PLUGINS_TO_INCLUDE: &str =
        "Specify a list of language service plugins to include.";
    pub const DISABLE_ERASING_CONST_ENUM_DECLARATIONS_IN_GENERATED_CODE: &str =
        "Disable erasing 'const enum' declarations in generated code.";
    pub const DISABLE_RESOLVING_SYMLINKS_TO_THEIR_REALPATH_THIS_CORRELATES_TO_THE_SAME_FLAG_IN:
        &str =
        "Disable resolving symlinks to their realpath. This correlates to the same flag in node.";
    pub const DISABLE_WIPING_THE_CONSOLE_IN_WATCH_MODE: &str =
        "Disable wiping the console in watch mode.";
    pub const ENABLE_COLOR_AND_FORMATTING_IN_TYPESCRIPTS_OUTPUT_TO_MAKE_COMPILER_ERRORS_EASIER:
        &str = "Enable color and formatting in TypeScript's output to make compiler errors easier to read.";
    pub const SPECIFY_THE_OBJECT_INVOKED_FOR_CREATEELEMENT_THIS_ONLY_APPLIES_WHEN_TARGETING_RE:
        &str = "Specify the object invoked for 'createElement'. This only applies when targeting 'react' JSX emit.";
    pub const SPECIFY_AN_ARRAY_OF_OBJECTS_THAT_SPECIFY_PATHS_FOR_PROJECTS_USED_IN_PROJECT_REFE:
        &str =
        "Specify an array of objects that specify paths for projects. Used in project references.";
    pub const DISABLE_EMITTING_COMMENTS: &str = "Disable emitting comments.";
    pub const ENABLE_IMPORTING_JSON_FILES: &str = "Enable importing .json files.";
    pub const SPECIFY_THE_ROOT_FOLDER_WITHIN_YOUR_SOURCE_FILES: &str =
        "Specify the root folder within your source files.";
    pub const ALLOW_MULTIPLE_FOLDERS_TO_BE_TREATED_AS_ONE_WHEN_RESOLVING_MODULES: &str =
        "Allow multiple folders to be treated as one when resolving modules.";
    pub const SKIP_TYPE_CHECKING_D_TS_FILES_THAT_ARE_INCLUDED_WITH_TYPESCRIPT: &str =
        "Skip type checking .d.ts files that are included with TypeScript.";
    pub const SKIP_TYPE_CHECKING_ALL_D_TS_FILES: &str = "Skip type checking all .d.ts files.";
    pub const CREATE_SOURCE_MAP_FILES_FOR_EMITTED_JAVASCRIPT_FILES: &str =
        "Create source map files for emitted JavaScript files.";
    pub const SPECIFY_THE_ROOT_PATH_FOR_DEBUGGERS_TO_FIND_THE_REFERENCE_SOURCE_CODE: &str =
        "Specify the root path for debuggers to find the reference source code.";
    pub const CHECK_THAT_THE_ARGUMENTS_FOR_BIND_CALL_AND_APPLY_METHODS_MATCH_THE_ORIGINAL_FUNC:
        &str = "Check that the arguments for 'bind', 'call', and 'apply' methods match the original function.";
    pub const WHEN_ASSIGNING_FUNCTIONS_CHECK_TO_ENSURE_PARAMETERS_AND_THE_RETURN_VALUES_ARE_SU:
        &str = "When assigning functions, check to ensure parameters and the return values are subtype-compatible.";
    pub const WHEN_TYPE_CHECKING_TAKE_INTO_ACCOUNT_NULL_AND_UNDEFINED: &str =
        "When type checking, take into account 'null' and 'undefined'.";
    pub const CHECK_FOR_CLASS_PROPERTIES_THAT_ARE_DECLARED_BUT_NOT_SET_IN_THE_CONSTRUCTOR: &str =
        "Check for class properties that are declared but not set in the constructor.";
    pub const DISABLE_EMITTING_DECLARATIONS_THAT_HAVE_INTERNAL_IN_THEIR_JSDOC_COMMENTS: &str =
        "Disable emitting declarations that have '@internal' in their JSDoc comments.";
    pub const DISABLE_REPORTING_OF_EXCESS_PROPERTY_ERRORS_DURING_THE_CREATION_OF_OBJECT_LITERA:
        &str =
        "Disable reporting of excess property errors during the creation of object literals.";
    pub const SUPPRESS_NOIMPLICITANY_ERRORS_WHEN_INDEXING_OBJECTS_THAT_LACK_INDEX_SIGNATURES: &str =
        "Suppress 'noImplicitAny' errors when indexing objects that lack index signatures.";
    pub const SYNCHRONOUSLY_CALL_CALLBACKS_AND_UPDATE_THE_STATE_OF_DIRECTORY_WATCHERS_ON_PLATF:
        &str = "Synchronously call callbacks and update the state of directory watchers on platforms that don`t support recursive watching natively.";
    pub const SET_THE_JAVASCRIPT_LANGUAGE_VERSION_FOR_EMITTED_JAVASCRIPT_AND_INCLUDE_COMPATIBL:
        &str = "Set the JavaScript language version for emitted JavaScript and include compatible library declarations.";
    pub const LOG_PATHS_USED_DURING_THE_MODULERESOLUTION_PROCESS: &str =
        "Log paths used during the 'moduleResolution' process.";
    pub const SPECIFY_THE_PATH_TO_TSBUILDINFO_INCREMENTAL_COMPILATION_FILE: &str =
        "Specify the path to .tsbuildinfo incremental compilation file.";
    pub const SPECIFY_OPTIONS_FOR_AUTOMATIC_ACQUISITION_OF_DECLARATION_FILES: &str =
        "Specify options for automatic acquisition of declaration files.";
    pub const SPECIFY_MULTIPLE_FOLDERS_THAT_ACT_LIKE_NODE_MODULES_TYPES: &str =
        "Specify multiple folders that act like './node_modules/@types'.";
    pub const SPECIFY_TYPE_PACKAGE_NAMES_TO_BE_INCLUDED_WITHOUT_BEING_REFERENCED_IN_A_SOURCE_F:
        &str =
        "Specify type package names to be included without being referenced in a source file.";
    pub const EMIT_ECMASCRIPT_STANDARD_COMPLIANT_CLASS_FIELDS: &str =
        "Emit ECMAScript-standard-compliant class fields.";
    pub const ENABLE_VERBOSE_LOGGING: &str = "Enable verbose logging.";
    pub const SPECIFY_HOW_DIRECTORIES_ARE_WATCHED_ON_SYSTEMS_THAT_LACK_RECURSIVE_FILE_WATCHING:
        &str = "Specify how directories are watched on systems that lack recursive file-watching functionality.";
    pub const SPECIFY_HOW_THE_TYPESCRIPT_WATCH_MODE_WORKS: &str =
        "Specify how the TypeScript watch mode works.";
    pub const REQUIRE_UNDECLARED_PROPERTIES_FROM_INDEX_SIGNATURES_TO_USE_ELEMENT_ACCESSES: &str =
        "Require undeclared properties from index signatures to use element accesses.";
    pub const SPECIFY_EMIT_CHECKING_BEHAVIOR_FOR_IMPORTS_THAT_ARE_ONLY_USED_FOR_TYPES: &str =
        "Specify emit/checking behavior for imports that are only used for types.";
    pub const REQUIRE_SUFFICIENT_ANNOTATION_ON_EXPORTS_SO_OTHER_TOOLS_CAN_TRIVIALLY_GENERATE_D:
        &str = "Require sufficient annotation on exports so other tools can trivially generate declaration files.";
    pub const BUILT_IN_ITERATORS_ARE_INSTANTIATED_WITH_A_TRETURN_TYPE_OF_UNDEFINED_INSTEAD_OF:
        &str = "Built-in iterators are instantiated with a 'TReturn' type of 'undefined' instead of 'any'.";
    pub const DO_NOT_ALLOW_RUNTIME_CONSTRUCTS_THAT_ARE_NOT_PART_OF_ECMASCRIPT: &str =
        "Do not allow runtime constructs that are not part of ECMAScript.";
    pub const DEFAULT_CATCH_CLAUSE_VARIABLES_AS_UNKNOWN_INSTEAD_OF_ANY: &str =
        "Default catch clause variables as 'unknown' instead of 'any'.";
    pub const DO_NOT_TRANSFORM_OR_ELIDE_ANY_IMPORTS_OR_EXPORTS_NOT_MARKED_AS_TYPE_ONLY_ENSURIN:
        &str = "Do not transform or elide any imports or exports not marked as type-only, ensuring they are written in the output file's format based on the 'module' setting.";
    pub const DISABLE_FULL_TYPE_CHECKING_ONLY_CRITICAL_PARSE_AND_EMIT_ERRORS_WILL_BE_REPORTED:
        &str = "Disable full type checking (only critical parse and emit errors will be reported).";
    pub const CHECK_SIDE_EFFECT_IMPORTS: &str = "Check side effect imports.";
    pub const THIS_OPERATION_CAN_BE_SIMPLIFIED_THIS_SHIFT_IS_IDENTICAL_TO: &str =
        "This operation can be simplified. This shift is identical to `{0} {1} {2}`.";
    pub const ENABLE_LIB_REPLACEMENT: &str = "Enable lib replacement.";
    pub const ENSURE_TYPES_ARE_ORDERED_STABLY_AND_DETERMINISTICALLY_ACROSS_COMPILATIONS: &str =
        "Ensure types are ordered stably and deterministically across compilations.";
    pub const ONE_OF: &str = "one of:";
    pub const ONE_OR_MORE: &str = "one or more:";
    pub const TYPE: &str = "type:";
    pub const DEFAULT: &str = "default:";
    pub const TRUE_UNLESS_STRICT_IS_FALSE: &str = "`true`, unless `strict` is `false`";
    pub const FALSE_UNLESS_COMPOSITE_IS_SET: &str = "`false`, unless `composite` is set";
    pub const NODE_MODULES_BOWER_COMPONENTS_JSPM_PACKAGES_PLUS_THE_VALUE_OF_OUTDIR_IF_ONE_IS_S:
        &str = "`[\"node_modules\", \"bower_components\", \"jspm_packages\"]`, plus the value of `outDir` if one is specified.";
    pub const IF_FILES_IS_SPECIFIED_OTHERWISE: &str =
        "`[]` if `files` is specified, otherwise `[\"**/*\"]`";
    pub const TRUE_IF_COMPOSITE_FALSE_OTHERWISE: &str = "`true` if `composite`, `false` otherwise";
    pub const COMPUTED_FROM_THE_LIST_OF_INPUT_FILES: &str = "Computed from the list of input files";
    pub const PLATFORM_SPECIFIC: &str = "Platform specific";
    pub const YOU_CAN_LEARN_ABOUT_ALL_OF_THE_COMPILER_OPTIONS_AT: &str =
        "You can learn about all of the compiler options at {0}";
    pub const INCLUDING_WATCH_W_WILL_START_WATCHING_THE_CURRENT_PROJECT_FOR_THE_FILE_CHANGES_O:
        &str = "Including --watch, -w will start watching the current project for the file changes. Once set, you can config watch mode with:";
    pub const USING_BUILD_B_WILL_MAKE_TSC_BEHAVE_MORE_LIKE_A_BUILD_ORCHESTRATOR_THAN_A_COMPILE:
        &str = "Using --build, -b will make tsc behave more like a build orchestrator than a compiler. This is used to trigger building composite projects which you can learn more about at {0}";
    pub const COMMON_COMMANDS: &str = "COMMON COMMANDS";
    pub const ALL_COMPILER_OPTIONS: &str = "ALL COMPILER OPTIONS";
    pub const WATCH_OPTIONS: &str = "WATCH OPTIONS";
    pub const BUILD_OPTIONS: &str = "BUILD OPTIONS";
    pub const COMMON_COMPILER_OPTIONS: &str = "COMMON COMPILER OPTIONS";
    pub const COMMAND_LINE_FLAGS: &str = "COMMAND LINE FLAGS";
    pub const TSC_THE_TYPESCRIPT_COMPILER: &str = "tsc: The TypeScript Compiler";
    pub const COMPILES_THE_CURRENT_PROJECT_TSCONFIG_JSON_IN_THE_WORKING_DIRECTORY: &str =
        "Compiles the current project (tsconfig.json in the working directory.)";
    pub const IGNORING_TSCONFIG_JSON_COMPILES_THE_SPECIFIED_FILES_WITH_DEFAULT_COMPILER_OPTION:
        &str =
        "Ignoring tsconfig.json, compiles the specified files with default compiler options.";
    pub const BUILD_A_COMPOSITE_PROJECT_IN_THE_WORKING_DIRECTORY: &str =
        "Build a composite project in the working directory.";
    pub const CREATES_A_TSCONFIG_JSON_WITH_THE_RECOMMENDED_SETTINGS_IN_THE_WORKING_DIRECTORY: &str =
        "Creates a tsconfig.json with the recommended settings in the working directory.";
    pub const COMPILES_THE_TYPESCRIPT_PROJECT_LOCATED_AT_THE_SPECIFIED_PATH: &str =
        "Compiles the TypeScript project located at the specified path.";
    pub const AN_EXPANDED_VERSION_OF_THIS_INFORMATION_SHOWING_ALL_POSSIBLE_COMPILER_OPTIONS: &str =
        "An expanded version of this information, showing all possible compiler options";
    pub const COMPILES_THE_CURRENT_PROJECT_WITH_ADDITIONAL_SETTINGS: &str =
        "Compiles the current project, with additional settings.";
    pub const TRUE_FOR_ES2022_AND_ABOVE_INCLUDING_ESNEXT: &str =
        "`true` for ES2022 and above, including ESNext.";
    pub const LIST_OF_FILE_NAME_SUFFIXES_TO_SEARCH_WHEN_RESOLVING_A_MODULE: &str =
        "List of file name suffixes to search when resolving a module.";
    pub const FALSE_UNLESS_CHECKJS_IS_SET: &str = "`false`, unless `checkJs` is set";
    pub const VARIABLE_IMPLICITLY_HAS_AN_TYPE: &str =
        "Variable '{0}' implicitly has an '{1}' type.";
    pub const PARAMETER_IMPLICITLY_HAS_AN_TYPE: &str =
        "Parameter '{0}' implicitly has an '{1}' type.";
    pub const MEMBER_IMPLICITLY_HAS_AN_TYPE: &str = "Member '{0}' implicitly has an '{1}' type.";
    pub const NEW_EXPRESSION_WHOSE_TARGET_LACKS_A_CONSTRUCT_SIGNATURE_IMPLICITLY_HAS_AN_ANY_TY:
        &str =
        "'new' expression, whose target lacks a construct signature, implicitly has an 'any' type.";
    pub const WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE: &str =
        "'{0}', which lacks return-type annotation, implicitly has an '{1}' return type.";
    pub const FUNCTION_EXPRESSION_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN:
        &str = "Function expression, which lacks return-type annotation, implicitly has an '{0}' return type.";
    pub const THIS_OVERLOAD_IMPLICITLY_RETURNS_THE_TYPE_BECAUSE_IT_LACKS_A_RETURN_TYPE_ANNOTAT:
        &str = "This overload implicitly returns the type '{0}' because it lacks a return type annotation.";
    pub const CONSTRUCT_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RET:
        &str = "Construct signature, which lacks return-type annotation, implicitly has an 'any' return type.";
    pub const FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE: &str =
        "Function type, which lacks return-type annotation, implicitly has an '{0}' return type.";
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE:
        &str =
        "Element implicitly has an 'any' type because index expression is not of type 'number'.";
    pub const COULD_NOT_FIND_A_DECLARATION_FILE_FOR_MODULE_IMPLICITLY_HAS_AN_ANY_TYPE: &str =
        "Could not find a declaration file for module '{0}'. '{1}' implicitly has an 'any' type.";
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE: &str =
        "Element implicitly has an 'any' type because type '{0}' has no index signature.";
    pub const OBJECT_LITERALS_PROPERTY_IMPLICITLY_HAS_AN_TYPE: &str =
        "Object literal's property '{0}' implicitly has an '{1}' type.";
    pub const REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE: &str =
        "Rest parameter '{0}' implicitly has an 'any[]' type.";
    pub const CALL_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RETURN_T:
        &str =
        "Call signature, which lacks return-type annotation, implicitly has an 'any' return type.";
    pub const IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE:
        &str = "'{0}' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.";
    pub const IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION:
        &str = "'{0}' implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.";
    pub const FUNCTION_IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_A:
        &str = "Function implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.";
    pub const GENERATOR_IMPLICITLY_HAS_YIELD_TYPE_CONSIDER_SUPPLYING_A_RETURN_TYPE_ANNOTATION:
        &str =
        "Generator implicitly has yield type '{0}'. Consider supplying a return type annotation.";
    pub const JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS: &str =
        "JSX element implicitly has type 'any' because no interface 'JSX.{0}' exists.";
    pub const UNREACHABLE_CODE_DETECTED: &str = "Unreachable code detected.";
    pub const UNUSED_LABEL: &str = "Unused label.";
    pub const FALLTHROUGH_CASE_IN_SWITCH: &str = "Fallthrough case in switch.";
    pub const NOT_ALL_CODE_PATHS_RETURN_A_VALUE: &str = "Not all code paths return a value.";
    pub const BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE: &str =
        "Binding element '{0}' implicitly has an '{1}' type.";
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE:
        &str = "Property '{0}' implicitly has type 'any', because its set accessor lacks a parameter type annotation.";
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_GET_ACCESSOR_LACKS_A_RETURN_TYPE_AN:
        &str = "Property '{0}' implicitly has type 'any', because its get accessor lacks a return type annotation.";
    pub const VARIABLE_IMPLICITLY_HAS_TYPE_IN_SOME_LOCATIONS_WHERE_ITS_TYPE_CANNOT_BE_DETERMIN:
        &str = "Variable '{0}' implicitly has type '{1}' in some locations where its type cannot be determined.";
    pub const TRY_NPM_I_SAVE_DEV_TYPES_IF_IT_EXISTS_OR_ADD_A_NEW_DECLARATION_D_TS_FILE_CONTAIN:
        &str = "Try `npm i --save-dev @types/{1}` if it exists or add a new declaration (.d.ts) file containing `declare module '{0}';`";
    pub const DYNAMIC_IMPORTS_SPECIFIER_MUST_BE_OF_TYPE_STRING_BUT_HERE_HAS_TYPE: &str =
        "Dynamic import's specifier must be of type 'string', but here has type '{0}'.";
    pub const ENABLES_EMIT_INTEROPERABILITY_BETWEEN_COMMONJS_AND_ES_MODULES_VIA_CREATION_OF_NA:
        &str = "Enables emit interoperability between CommonJS and ES Modules via creation of namespace objects for all imports. Implies 'allowSyntheticDefaultImports'.";
    pub const TYPE_ORIGINATES_AT_THIS_IMPORT_A_NAMESPACE_STYLE_IMPORT_CANNOT_BE_CALLED_OR_CONS:
        &str = "Type originates at this import. A namespace-style import cannot be called or constructed, and will cause a failure at runtime. Consider using a default import or import require here instead.";
    pub const MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE: &str =
        "Mapped object type implicitly has an 'any' template type.";
    pub const IF_THE_PACKAGE_ACTUALLY_EXPOSES_THIS_MODULE_CONSIDER_SENDING_A_PULL_REQUEST_TO_A:
        &str = "If the '{0}' package actually exposes this module, consider sending a pull request to amend 'https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/{1}'";
    pub const THE_CONTAINING_ARROW_FUNCTION_CAPTURES_THE_GLOBAL_VALUE_OF_THIS: &str =
        "The containing arrow function captures the global value of 'this'.";
    pub const MODULE_WAS_RESOLVED_TO_BUT_RESOLVEJSONMODULE_IS_NOT_USED: &str =
        "Module '{0}' was resolved to '{1}', but '--resolveJsonModule' is not used.";
    pub const VARIABLE_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: &str = "Variable '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.";
    pub const PARAMETER_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: &str = "Parameter '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.";
    pub const MEMBER_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: &str =
        "Member '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.";
    pub const VARIABLE_IMPLICITLY_HAS_TYPE_IN_SOME_LOCATIONS_BUT_A_BETTER_TYPE_MAY_BE_INFERRED:
        &str = "Variable '{0}' implicitly has type '{1}' in some locations, but a better type may be inferred from usage.";
    pub const REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM:
        &str = "Rest parameter '{0}' implicitly has an 'any[]' type, but a better type may be inferred from usage.";
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BUT_A_BETTER_TYPE_FOR_ITS_GET_ACCESSOR_MAY_BE_I:
        &str = "Property '{0}' implicitly has type 'any', but a better type for its get accessor may be inferred from usage.";
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BUT_A_BETTER_TYPE_FOR_ITS_SET_ACCESSOR_MAY_BE_I:
        &str = "Property '{0}' implicitly has type 'any', but a better type for its set accessor may be inferred from usage.";
    pub const IMPLICITLY_HAS_AN_RETURN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: &str =
        "'{0}' implicitly has an '{1}' return type, but a better type may be inferred from usage.";
    pub const PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN: &str =
        "Parameter has a name but no type. Did you mean '{0}: {1}'?";
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE_DID_YOU_M:
        &str = "Element implicitly has an 'any' type because type '{0}' has no index signature. Did you mean to call '{1}'?";
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN:
        &str = "Element implicitly has an 'any' type because expression of type '{0}' can't be used to index type '{1}'.";
    pub const NO_INDEX_SIGNATURE_WITH_A_PARAMETER_OF_TYPE_WAS_FOUND_ON_TYPE: &str =
        "No index signature with a parameter of type '{0}' was found on type '{1}'.";
    pub const WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_YIELD_TYPE: &str =
        "'{0}', which lacks return-type annotation, implicitly has an '{1}' yield type.";
    pub const THE_INFERRED_TYPE_OF_THIS_NODE_EXCEEDS_THE_MAXIMUM_LENGTH_THE_COMPILER_WILL_SERI:
        &str = "The inferred type of this node exceeds the maximum length the compiler will serialize. An explicit type annotation is needed.";
    pub const YIELD_EXPRESSION_IMPLICITLY_RESULTS_IN_AN_ANY_TYPE_BECAUSE_ITS_CONTAINING_GENERA:
        &str = "'yield' expression implicitly results in an 'any' type because its containing generator lacks a return-type annotation.";
    pub const IF_THE_PACKAGE_ACTUALLY_EXPOSES_THIS_MODULE_TRY_ADDING_A_NEW_DECLARATION_D_TS_FI:
        &str = "If the '{0}' package actually exposes this module, try adding a new declaration (.d.ts) file containing `declare module '{1}';`";
    pub const THIS_SYNTAX_IS_RESERVED_IN_FILES_WITH_THE_MTS_OR_CTS_EXTENSION_USE_AN_AS_EXPRESS:
        &str = "This syntax is reserved in files with the .mts or .cts extension. Use an `as` expression instead.";
    pub const THIS_SYNTAX_IS_RESERVED_IN_FILES_WITH_THE_MTS_OR_CTS_EXTENSION_ADD_A_TRAILING_CO:
        &str = "This syntax is reserved in files with the .mts or .cts extension. Add a trailing comma or explicit constraint.";
    pub const A_MAPPED_TYPE_MAY_NOT_DECLARE_PROPERTIES_OR_METHODS: &str =
        "A mapped type may not declare properties or methods.";
    pub const YOU_CANNOT_RENAME_THIS_ELEMENT: &str = "You cannot rename this element.";
    pub const YOU_CANNOT_RENAME_ELEMENTS_THAT_ARE_DEFINED_IN_THE_STANDARD_TYPESCRIPT_LIBRARY: &str =
        "You cannot rename elements that are defined in the standard TypeScript library.";
    pub const IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "'import ... =' can only be used in TypeScript files.";
    pub const EXPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "'export =' can only be used in TypeScript files.";
    pub const TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Type parameter declarations can only be used in TypeScript files.";
    pub const IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "'implements' clauses can only be used in TypeScript files.";
    pub const DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "'{0}' declarations can only be used in TypeScript files.";
    pub const TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Type aliases can only be used in TypeScript files.";
    pub const THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "The '{0}' modifier can only be used in TypeScript files.";
    pub const TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Type annotations can only be used in TypeScript files.";
    pub const TYPE_ARGUMENTS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Type arguments can only be used in TypeScript files.";
    pub const PARAMETER_MODIFIERS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Parameter modifiers can only be used in TypeScript files.";
    pub const NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Non-null assertions can only be used in TypeScript files.";
    pub const TYPE_ASSERTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Type assertion expressions can only be used in TypeScript files.";
    pub const SIGNATURE_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Signature declarations can only be used in TypeScript files.";
    pub const REPORT_ERRORS_IN_JS_FILES: &str = "Report errors in .js files.";
    pub const JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS: &str =
        "JSDoc types can only be used inside documentation comments.";
    pub const JSDOC_TYPEDEF_TAG_SHOULD_EITHER_HAVE_A_TYPE_ANNOTATION_OR_BE_FOLLOWED_BY_PROPERT:
        &str = "JSDoc '@typedef' tag should either have a type annotation or be followed by '@property' or '@member' tags.";
    pub const JSDOC_IS_NOT_ATTACHED_TO_A_CLASS: &str = "JSDoc '@{0}' is not attached to a class.";
    pub const JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE: &str =
        "JSDoc '@{0} {1}' does not match the 'extends {2}' clause.";
    pub const JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME: &str =
        "JSDoc '@param' tag has name '{0}', but there is no parameter with that name.";
    pub const CLASS_DECLARATIONS_CANNOT_HAVE_MORE_THAN_ONE_AUGMENTS_OR_EXTENDS_TAG: &str =
        "Class declarations cannot have more than one '@augments' or '@extends' tag.";
    pub const EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG: &str =
        "Expected {0} type arguments; provide these with an '@extends' tag.";
    pub const EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG_2: &str =
        "Expected {0}-{1} type arguments; provide these with an '@extends' tag.";
    pub const JSDOC_MAY_ONLY_APPEAR_IN_THE_LAST_PARAMETER_OF_A_SIGNATURE: &str =
        "JSDoc '...' may only appear in the last parameter of a signature.";
    pub const JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME_IT_WOULD_MATCH:
        &str = "JSDoc '@param' tag has name '{0}', but there is no parameter with that name. It would match 'arguments' if it had an array type.";
    pub const THE_TYPE_OF_A_FUNCTION_DECLARATION_MUST_MATCH_THE_FUNCTIONS_SIGNATURE: &str =
        "The type of a function declaration must match the function's signature.";
    pub const YOU_CANNOT_RENAME_A_MODULE_VIA_A_GLOBAL_IMPORT: &str =
        "You cannot rename a module via a global import.";
    pub const QUALIFIED_NAME_IS_NOT_ALLOWED_WITHOUT_A_LEADING_PARAM_OBJECT: &str =
        "Qualified name '{0}' is not allowed without a leading '@param {object} {1}'.";
    pub const A_JSDOC_TYPEDEF_COMMENT_MAY_NOT_CONTAIN_MULTIPLE_TYPE_TAGS: &str =
        "A JSDoc '@typedef' comment may not contain multiple '@type' tags.";
    pub const THE_TAG_WAS_FIRST_SPECIFIED_HERE: &str = "The tag was first specified here.";
    pub const YOU_CANNOT_RENAME_ELEMENTS_THAT_ARE_DEFINED_IN_A_NODE_MODULES_FOLDER: &str =
        "You cannot rename elements that are defined in a 'node_modules' folder.";
    pub const YOU_CANNOT_RENAME_ELEMENTS_THAT_ARE_DEFINED_IN_ANOTHER_NODE_MODULES_FOLDER: &str =
        "You cannot rename elements that are defined in another 'node_modules' folder.";
    pub const TYPE_SATISFACTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: &str =
        "Type satisfaction expressions can only be used in TypeScript files.";
    pub const DECORATORS_MAY_NOT_APPEAR_AFTER_EXPORT_OR_EXPORT_DEFAULT_IF_THEY_ALSO_APPEAR_BEF:
        &str = "Decorators may not appear after 'export' or 'export default' if they also appear before 'export'.";
    pub const A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG: &str =
        "A JSDoc '@template' tag may not follow a '@typedef', '@callback', or '@overload' tag";
    pub const DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_USING_PRIVATE_NAME_AN_EXPLICIT_TYPE_ANNO:
        &str = "Declaration emit for this file requires using private name '{0}'. An explicit type annotation may unblock declaration emit.";
    pub const DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_USING_PRIVATE_NAME_FROM_MODULE_AN_EXPLIC:
        &str = "Declaration emit for this file requires using private name '{0}' from module '{1}'. An explicit type annotation may unblock declaration emit.";
    pub const FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS:
        &str = "Function must have an explicit return type annotation with --isolatedDeclarations.";
    pub const METHOD_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: &str =
        "Method must have an explicit return type annotation with --isolatedDeclarations.";
    pub const AT_LEAST_ONE_ACCESSOR_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARA:
        &str =
        "At least one accessor must have an explicit type annotation with --isolatedDeclarations.";
    pub const VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: &str =
        "Variable must have an explicit type annotation with --isolatedDeclarations.";
    pub const PARAMETER_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: &str =
        "Parameter must have an explicit type annotation with --isolatedDeclarations.";
    pub const PROPERTY_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: &str =
        "Property must have an explicit type annotation with --isolatedDeclarations.";
    pub const EXPRESSION_TYPE_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS: &str =
        "Expression type can't be inferred with --isolatedDeclarations.";
    pub const COMPUTED_PROPERTIES_MUST_BE_NUMBER_OR_STRING_LITERALS_VARIABLES_OR_DOTTED_EXPRES:
        &str = "Computed properties must be number or string literals, variables or dotted expressions with --isolatedDeclarations.";
    pub const OBJECTS_THAT_CONTAIN_SPREAD_ASSIGNMENTS_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIO:
        &str =
        "Objects that contain spread assignments can't be inferred with --isolatedDeclarations.";
    pub const OBJECTS_THAT_CONTAIN_SHORTHAND_PROPERTIES_CANT_BE_INFERRED_WITH_ISOLATEDDECLARAT:
        &str =
        "Objects that contain shorthand properties can't be inferred with --isolatedDeclarations.";
    pub const ONLY_CONST_ARRAYS_CAN_BE_INFERRED_WITH_ISOLATEDDECLARATIONS: &str =
        "Only const arrays can be inferred with --isolatedDeclarations.";
    pub const ARRAYS_WITH_SPREAD_ELEMENTS_CANT_INFERRED_WITH_ISOLATEDDECLARATIONS: &str =
        "Arrays with spread elements can't inferred with --isolatedDeclarations.";
    pub const BINDING_ELEMENTS_CANT_BE_EXPORTED_DIRECTLY_WITH_ISOLATEDDECLARATIONS: &str =
        "Binding elements can't be exported directly with --isolatedDeclarations.";
    pub const ENUM_MEMBER_INITIALIZERS_MUST_BE_COMPUTABLE_WITHOUT_REFERENCES_TO_EXTERNAL_SYMBO:
        &str = "Enum member initializers must be computable without references to external symbols with --isolatedDeclarations.";
    pub const EXTENDS_CLAUSE_CANT_CONTAIN_AN_EXPRESSION_WITH_ISOLATEDDECLARATIONS: &str =
        "Extends clause can't contain an expression with --isolatedDeclarations.";
    pub const INFERENCE_FROM_CLASS_EXPRESSIONS_IS_NOT_SUPPORTED_WITH_ISOLATEDDECLARATIONS: &str =
        "Inference from class expressions is not supported with --isolatedDeclarations.";
    pub const ASSIGNING_PROPERTIES_TO_FUNCTIONS_WITHOUT_DECLARING_THEM_IS_NOT_SUPPORTED_WITH_I:
        &str = "Assigning properties to functions without declaring them is not supported with --isolatedDeclarations. Add an explicit declaration for the properties assigned to this function.";
    pub const DECLARATION_EMIT_FOR_THIS_PARAMETER_REQUIRES_IMPLICITLY_ADDING_UNDEFINED_TO_ITS:
        &str = "Declaration emit for this parameter requires implicitly adding undefined to its type. This is not supported with --isolatedDeclarations.";
    pub const DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_PRESERVING_THIS_IMPORT_FOR_AUGMENTATIONS:
        &str = "Declaration emit for this file requires preserving this import for augmentations. This is not supported with --isolatedDeclarations.";
    pub const ADD_A_TYPE_ANNOTATION_TO_THE_VARIABLE: &str =
        "Add a type annotation to the variable {0}.";
    pub const ADD_A_TYPE_ANNOTATION_TO_THE_PARAMETER: &str =
        "Add a type annotation to the parameter {0}.";
    pub const ADD_A_TYPE_ANNOTATION_TO_THE_PROPERTY: &str =
        "Add a type annotation to the property {0}.";
    pub const ADD_A_RETURN_TYPE_TO_THE_FUNCTION_EXPRESSION: &str =
        "Add a return type to the function expression.";
    pub const ADD_A_RETURN_TYPE_TO_THE_FUNCTION_DECLARATION: &str =
        "Add a return type to the function declaration.";
    pub const ADD_A_RETURN_TYPE_TO_THE_GET_ACCESSOR_DECLARATION: &str =
        "Add a return type to the get accessor declaration.";
    pub const ADD_A_TYPE_TO_PARAMETER_OF_THE_SET_ACCESSOR_DECLARATION: &str =
        "Add a type to parameter of the set accessor declaration.";
    pub const ADD_A_RETURN_TYPE_TO_THE_METHOD: &str = "Add a return type to the method";
    pub const ADD_SATISFIES_AND_A_TYPE_ASSERTION_TO_THIS_EXPRESSION_SATISFIES_T_AS_T_TO_MAKE_T:
        &str = "Add satisfies and a type assertion to this expression (satisfies T as T) to make the type explicit.";
    pub const MOVE_THE_EXPRESSION_IN_DEFAULT_EXPORT_TO_A_VARIABLE_AND_ADD_A_TYPE_ANNOTATION_TO:
        &str =
        "Move the expression in default export to a variable and add a type annotation to it.";
    pub const DEFAULT_EXPORTS_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS: &str =
        "Default exports can't be inferred with --isolatedDeclarations.";
    pub const COMPUTED_PROPERTY_NAMES_ON_CLASS_OR_OBJECT_LITERALS_CANNOT_BE_INFERRED_WITH_ISOL:
        &str = "Computed property names on class or object literals cannot be inferred with --isolatedDeclarations.";
    pub const TYPE_CONTAINING_PRIVATE_NAME_CANT_BE_USED_WITH_ISOLATEDDECLARATIONS: &str =
        "Type containing private name '{0}' can't be used with --isolatedDeclarations.";
    pub const JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION: &str =
        "JSX attributes must only be assigned a non-empty 'expression'.";
    pub const JSX_ELEMENTS_CANNOT_HAVE_MULTIPLE_ATTRIBUTES_WITH_THE_SAME_NAME: &str =
        "JSX elements cannot have multiple attributes with the same name.";
    pub const EXPECTED_CORRESPONDING_JSX_CLOSING_TAG_FOR: &str =
        "Expected corresponding JSX closing tag for '{0}'.";
    pub const CANNOT_USE_JSX_UNLESS_THE_JSX_FLAG_IS_PROVIDED: &str =
        "Cannot use JSX unless the '--jsx' flag is provided.";
    pub const A_CONSTRUCTOR_CANNOT_CONTAIN_A_SUPER_CALL_WHEN_ITS_CLASS_EXTENDS_NULL: &str =
        "A constructor cannot contain a 'super' call when its class extends 'null'.";
    pub const AN_UNARY_EXPRESSION_WITH_THE_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN:
        &str = "An unary expression with the '{0}' operator is not allowed in the left-hand side of an exponentiation expression. Consider enclosing the expression in parentheses.";
    pub const A_TYPE_ASSERTION_EXPRESSION_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN_EXPONENTI:
        &str = "A type assertion expression is not allowed in the left-hand side of an exponentiation expression. Consider enclosing the expression in parentheses.";
    pub const JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG: &str =
        "JSX element '{0}' has no corresponding closing tag.";
    pub const SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS:
        &str =
        "'super' must be called before accessing 'this' in the constructor of a derived class.";
    pub const UNKNOWN_TYPE_ACQUISITION_OPTION: &str = "Unknown type acquisition option '{0}'.";
    pub const SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF:
        &str = "'super' must be called before accessing a property of 'super' in the constructor of a derived class.";
    pub const IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN: &str =
        "'{0}' is not a valid meta-property for keyword '{1}'. Did you mean '{2}'?";
    pub const META_PROPERTY_IS_ONLY_ALLOWED_IN_THE_BODY_OF_A_FUNCTION_DECLARATION_FUNCTION_EXP:
        &str = "Meta-property '{0}' is only allowed in the body of a function declaration, function expression, or constructor.";
    pub const JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG: &str =
        "JSX fragment has no corresponding closing tag.";
    pub const EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT: &str =
        "Expected corresponding closing tag for JSX fragment.";
    pub const THE_JSXFRAGMENTFACTORY_COMPILER_OPTION_MUST_BE_PROVIDED_TO_USE_JSX_FRAGMENTS_WIT:
        &str = "The 'jsxFragmentFactory' compiler option must be provided to use JSX fragments with the 'jsxFactory' compiler option.";
    pub const AN_JSXFRAG_PRAGMA_IS_REQUIRED_WHEN_USING_AN_JSX_PRAGMA_WITH_JSX_FRAGMENTS: &str =
        "An @jsxFrag pragma is required when using an @jsx pragma with JSX fragments.";
    pub const UNKNOWN_TYPE_ACQUISITION_OPTION_DID_YOU_MEAN: &str =
        "Unknown type acquisition option '{0}'. Did you mean '{1}'?";
    pub const AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE: &str =
        "'{0}' at the end of a type is not valid TypeScript syntax. Did you mean to write '{1}'?";
    pub const AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE: &str =
        "'{0}' at the start of a type is not valid TypeScript syntax. Did you mean to write '{1}'?";
    pub const UNICODE_ESCAPE_SEQUENCE_CANNOT_APPEAR_HERE: &str =
        "Unicode escape sequence cannot appear here.";
    pub const CIRCULARITY_DETECTED_WHILE_RESOLVING_CONFIGURATION: &str =
        "Circularity detected while resolving configuration: {0}";
    pub const THE_FILES_LIST_IN_CONFIG_FILE_IS_EMPTY: &str =
        "The 'files' list in config file '{0}' is empty.";
    pub const NO_INPUTS_WERE_FOUND_IN_CONFIG_FILE_SPECIFIED_INCLUDE_PATHS_WERE_AND_EXCLUDE_PAT:
        &str = "No inputs were found in config file '{0}'. Specified 'include' paths were '{1}' and 'exclude' paths were '{2}'.";
    pub const NO_VALUE_EXISTS_IN_SCOPE_FOR_THE_SHORTHAND_PROPERTY_EITHER_DECLARE_ONE_OR_PROVID:
        &str = "No value exists in scope for the shorthand property '{0}'. Either declare one or provide an initializer.";
    pub const CLASSES_MAY_NOT_HAVE_A_FIELD_NAMED_CONSTRUCTOR: &str =
        "Classes may not have a field named 'constructor'.";
    pub const JSX_EXPRESSIONS_MAY_NOT_USE_THE_COMMA_OPERATOR_DID_YOU_MEAN_TO_WRITE_AN_ARRAY: &str =
        "JSX expressions may not use the comma operator. Did you mean to write an array?";
    pub const PRIVATE_IDENTIFIERS_CANNOT_BE_USED_AS_PARAMETERS: &str =
        "Private identifiers cannot be used as parameters.";
    pub const AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER: &str =
        "An accessibility modifier cannot be used with a private identifier.";
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_PRIVATE_IDENTIFIER: &str =
        "The operand of a 'delete' operator cannot be a private identifier.";
    pub const CONSTRUCTOR_IS_A_RESERVED_WORD: &str = "'#constructor' is a reserved word.";
    pub const PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER: &str =
        "Property '{0}' is not accessible outside class '{1}' because it has a private identifier.";
    pub const THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED:
        &str = "The property '{0}' cannot be accessed on type '{1}' within this class because it is shadowed by another private identifier with the same spelling.";
    pub const PROPERTY_IN_TYPE_REFERS_TO_A_DIFFERENT_MEMBER_THAT_CANNOT_BE_ACCESSED_FROM_WITHI:
        &str = "Property '{0}' in type '{1}' refers to a different member that cannot be accessed from within type '{2}'.";
    pub const PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES: &str =
        "Private identifiers are not allowed outside class bodies.";
    pub const THE_SHADOWING_DECLARATION_OF_IS_DEFINED_HERE: &str =
        "The shadowing declaration of '{0}' is defined here";
    pub const THE_DECLARATION_OF_THAT_YOU_PROBABLY_INTENDED_TO_USE_IS_DEFINED_HERE: &str =
        "The declaration of '{0}' that you probably intended to use is defined here";
    pub const MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER: &str =
        "'{0}' modifier cannot be used with a private identifier.";
    pub const AN_ENUM_MEMBER_CANNOT_BE_NAMED_WITH_A_PRIVATE_IDENTIFIER: &str =
        "An enum member cannot be named with a private identifier.";
    pub const CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE: &str =
        "'#!' can only be used at the start of a file.";
    pub const COMPILER_RESERVES_NAME_WHEN_EMITTING_PRIVATE_IDENTIFIER_DOWNLEVEL: &str =
        "Compiler reserves name '{0}' when emitting private identifier downlevel.";
    pub const PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER:
        &str = "Private identifiers are only available when targeting ECMAScript 2015 and higher.";
    pub const PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_IN_VARIABLE_DECLARATIONS: &str =
        "Private identifiers are not allowed in variable declarations.";
    pub const AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS: &str =
        "An optional chain cannot contain private identifiers.";
    pub const THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN:
        &str = "The intersection '{0}' was reduced to 'never' because property '{1}' has conflicting types in some constituents.";
    pub const THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_EXISTS_IN_MULTIPLE_CONSTI:
        &str = "The intersection '{0}' was reduced to 'never' because property '{1}' exists in multiple constituents and is private in some.";
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_REQUIRED_FOR_COMPUTED_ENUM_MEMBER_VALUES: &str =
        "Type '{0}' is not assignable to type '{1}' as required for computed enum member values.";
    pub const SPECIFY_THE_JSX_FRAGMENT_FACTORY_FUNCTION_TO_USE_WHEN_TARGETING_REACT_JSX_EMIT_W:
        &str = "Specify the JSX fragment factory function to use when targeting 'react' JSX emit with 'jsxFactory' compiler option is specified, e.g. 'Fragment'.";
    pub const INVALID_VALUE_FOR_JSXFRAGMENTFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME:
        &str = "Invalid value for 'jsxFragmentFactory'. '{0}' is not a valid identifier or qualified-name.";
    pub const CLASS_DECORATORS_CANT_BE_USED_WITH_STATIC_PRIVATE_IDENTIFIER_CONSIDER_REMOVING_T:
        &str = "Class decorators can't be used with static private identifier. Consider removing the experimental decorator.";
    pub const AWAIT_EXPRESSION_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: &str =
        "'await' expression cannot be used inside a class static block.";
    pub const FOR_AWAIT_LOOPS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: &str =
        "'for await' loops cannot be used inside a class static block.";
    pub const INVALID_USE_OF_IT_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: &str =
        "Invalid use of '{0}'. It cannot be used inside a class static block.";
    pub const A_RETURN_STATEMENT_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: &str =
        "A 'return' statement cannot be used inside a class static block.";
    pub const IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT:
        &str = "'{0}' is a type and cannot be imported in JavaScript files. Use '{1}' in a JSDoc type annotation.";
    pub const TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES: &str =
        "Types cannot appear in export declarations in JavaScript files.";
    pub const IS_AUTOMATICALLY_EXPORTED_HERE: &str = "'{0}' is automatically exported here.";
    pub const PROPERTIES_WITH_THE_ACCESSOR_MODIFIER_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRI:
        &str = "Properties with the 'accessor' modifier are only available when targeting ECMAScript 2015 and higher.";
    pub const IS_OF_TYPE_UNKNOWN: &str = "'{0}' is of type 'unknown'.";
    pub const IS_POSSIBLY_NULL: &str = "'{0}' is possibly 'null'.";
    pub const IS_POSSIBLY_UNDEFINED: &str = "'{0}' is possibly 'undefined'.";
    pub const IS_POSSIBLY_NULL_OR_UNDEFINED: &str = "'{0}' is possibly 'null' or 'undefined'.";
    pub const THE_VALUE_CANNOT_BE_USED_HERE: &str = "The value '{0}' cannot be used here.";
    pub const COMPILER_OPTION_CANNOT_BE_GIVEN_AN_EMPTY_STRING: &str =
        "Compiler option '{0}' cannot be given an empty string.";
    pub const ITS_TYPE_IS_NOT_A_VALID_JSX_ELEMENT_TYPE: &str =
        "Its type '{0}' is not a valid JSX element type.";
    pub const AWAIT_USING_STATEMENTS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: &str =
        "'await using' statements cannot be used inside a class static block.";
    pub const HAS_A_STRING_TYPE_BUT_MUST_HAVE_SYNTACTICALLY_RECOGNIZABLE_STRING_SYNTAX_WHEN_IS:
        &str = "'{0}' has a string type, but must have syntactically recognizable string syntax when 'isolatedModules' is enabled.";
    pub const ENUM_MEMBER_FOLLOWING_A_NON_LITERAL_NUMERIC_MEMBER_MUST_HAVE_AN_INITIALIZER_WHEN:
        &str = "Enum member following a non-literal numeric member must have an initializer when 'isolatedModules' is enabled.";
    pub const STRING_LITERAL_IMPORT_AND_EXPORT_NAMES_ARE_NOT_SUPPORTED_WHEN_THE_MODULE_FLAG_IS:
        &str = "String literal import and export names are not supported when the '--module' flag is set to 'es2015' or 'es2020'.";
    pub const DEFAULT_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT: &str =
        "Default imports are not allowed in a deferred import.";
    pub const NAMED_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT: &str =
        "Named imports are not allowed in a deferred import.";
    pub const DEFERRED_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ESNEXT_OR_PRE:
        &str = "Deferred imports are only supported when the '--module' flag is set to 'esnext' or 'preserve'.";
    pub const IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_IMPORT_DID_YOU_MEAN_META_OR_DEFER: &str =
        "'{0}' is not a valid meta-property for keyword 'import'. Did you mean 'meta' or 'defer'?";
    pub const NODENEXT_IF_MODULE_IS_NODENEXT_NODE16_IF_MODULE_IS_NODE16_OR_NODE18_OTHERWISE_BU:
        &str = "`nodenext` if `module` is `nodenext`; `node16` if `module` is `node16` or `node18`; otherwise, `bundler`.";
    pub const FILE_IS_A_COMMONJS_MODULE_IT_MAY_BE_CONVERTED_TO_AN_ES_MODULE: &str =
        "File is a CommonJS module; it may be converted to an ES module.";
    pub const THIS_CONSTRUCTOR_FUNCTION_MAY_BE_CONVERTED_TO_A_CLASS_DECLARATION: &str =
        "This constructor function may be converted to a class declaration.";
    pub const IMPORT_MAY_BE_CONVERTED_TO_A_DEFAULT_IMPORT: &str =
        "Import may be converted to a default import.";
    pub const JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES: &str =
        "JSDoc types may be moved to TypeScript types.";
    pub const REQUIRE_CALL_MAY_BE_CONVERTED_TO_AN_IMPORT: &str =
        "'require' call may be converted to an import.";
    pub const THIS_MAY_BE_CONVERTED_TO_AN_ASYNC_FUNCTION: &str =
        "This may be converted to an async function.";
    pub const AWAIT_HAS_NO_EFFECT_ON_THE_TYPE_OF_THIS_EXPRESSION: &str =
        "'await' has no effect on the type of this expression.";
    pub const NUMERIC_LITERALS_WITH_ABSOLUTE_VALUES_EQUAL_TO_2_53_OR_GREATER_ARE_TOO_LARGE_TO:
        &str = "Numeric literals with absolute values equal to 2^53 or greater are too large to be represented accurately as integers.";
    pub const JSDOC_TYPEDEF_MAY_BE_CONVERTED_TO_TYPESCRIPT_TYPE: &str =
        "JSDoc typedef may be converted to TypeScript type.";
    pub const JSDOC_TYPEDEFS_MAY_BE_CONVERTED_TO_TYPESCRIPT_TYPES: &str =
        "JSDoc typedefs may be converted to TypeScript types.";
    pub const ADD_MISSING_SUPER_CALL: &str = "Add missing 'super()' call";
    pub const MAKE_SUPER_CALL_THE_FIRST_STATEMENT_IN_THE_CONSTRUCTOR: &str =
        "Make 'super()' call the first statement in the constructor";
    pub const CHANGE_EXTENDS_TO_IMPLEMENTS: &str = "Change 'extends' to 'implements'";
    pub const REMOVE_UNUSED_DECLARATION_FOR: &str = "Remove unused declaration for: '{0}'";
    pub const REMOVE_IMPORT_FROM: &str = "Remove import from '{0}'";
    pub const IMPLEMENT_INTERFACE: &str = "Implement interface '{0}'";
    pub const IMPLEMENT_INHERITED_ABSTRACT_CLASS: &str = "Implement inherited abstract class";
    pub const ADD_TO_UNRESOLVED_VARIABLE: &str = "Add '{0}.' to unresolved variable";
    pub const REMOVE_VARIABLE_STATEMENT: &str = "Remove variable statement";
    pub const REMOVE_TEMPLATE_TAG: &str = "Remove template tag";
    pub const REMOVE_TYPE_PARAMETERS: &str = "Remove type parameters";
    pub const IMPORT_FROM: &str = "Import '{0}' from \"{1}\"";
    pub const CHANGE_TO: &str = "Change '{0}' to '{1}'";
    pub const DECLARE_PROPERTY: &str = "Declare property '{0}'";
    pub const ADD_INDEX_SIGNATURE_FOR_PROPERTY: &str = "Add index signature for property '{0}'";
    pub const DISABLE_CHECKING_FOR_THIS_FILE: &str = "Disable checking for this file";
    pub const IGNORE_THIS_ERROR_MESSAGE: &str = "Ignore this error message";
    pub const INITIALIZE_PROPERTY_IN_THE_CONSTRUCTOR: &str =
        "Initialize property '{0}' in the constructor";
    pub const INITIALIZE_STATIC_PROPERTY: &str = "Initialize static property '{0}'";
    pub const CHANGE_SPELLING_TO: &str = "Change spelling to '{0}'";
    pub const DECLARE_METHOD: &str = "Declare method '{0}'";
    pub const DECLARE_STATIC_METHOD: &str = "Declare static method '{0}'";
    pub const PREFIX_WITH_AN_UNDERSCORE: &str = "Prefix '{0}' with an underscore";
    pub const REWRITE_AS_THE_INDEXED_ACCESS_TYPE: &str = "Rewrite as the indexed access type '{0}'";
    pub const DECLARE_STATIC_PROPERTY: &str = "Declare static property '{0}'";
    pub const CALL_DECORATOR_EXPRESSION: &str = "Call decorator expression";
    pub const ADD_ASYNC_MODIFIER_TO_CONTAINING_FUNCTION: &str =
        "Add async modifier to containing function";
    pub const REPLACE_INFER_WITH_UNKNOWN: &str = "Replace 'infer {0}' with 'unknown'";
    pub const REPLACE_ALL_UNUSED_INFER_WITH_UNKNOWN: &str =
        "Replace all unused 'infer' with 'unknown'";
    pub const ADD_PARAMETER_NAME: &str = "Add parameter name";
    pub const DECLARE_PRIVATE_PROPERTY: &str = "Declare private property '{0}'";
    pub const REPLACE_WITH_PROMISE: &str = "Replace '{0}' with 'Promise<{1}>'";
    pub const FIX_ALL_INCORRECT_RETURN_TYPE_OF_AN_ASYNC_FUNCTIONS: &str =
        "Fix all incorrect return type of an async functions";
    pub const DECLARE_PRIVATE_METHOD: &str = "Declare private method '{0}'";
    pub const REMOVE_UNUSED_DESTRUCTURING_DECLARATION: &str =
        "Remove unused destructuring declaration";
    pub const REMOVE_UNUSED_DECLARATIONS_FOR: &str = "Remove unused declarations for: '{0}'";
    pub const DECLARE_A_PRIVATE_FIELD_NAMED: &str = "Declare a private field named '{0}'.";
    pub const INCLUDES_IMPORTS_OF_TYPES_REFERENCED_BY: &str =
        "Includes imports of types referenced by '{0}'";
    pub const REMOVE_TYPE_FROM_IMPORT_DECLARATION_FROM: &str =
        "Remove 'type' from import declaration from \"{0}\"";
    pub const REMOVE_TYPE_FROM_IMPORT_OF_FROM: &str =
        "Remove 'type' from import of '{0}' from \"{1}\"";
    pub const ADD_IMPORT_FROM: &str = "Add import from \"{0}\"";
    pub const UPDATE_IMPORT_FROM: &str = "Update import from \"{0}\"";
    pub const EXPORT_FROM_MODULE: &str = "Export '{0}' from module '{1}'";
    pub const EXPORT_ALL_REFERENCED_LOCALS: &str = "Export all referenced locals";
    pub const UPDATE_MODIFIERS_OF: &str = "Update modifiers of '{0}'";
    pub const ADD_ANNOTATION_OF_TYPE: &str = "Add annotation of type '{0}'";
    pub const ADD_RETURN_TYPE: &str = "Add return type '{0}'";
    pub const EXTRACT_BASE_CLASS_TO_VARIABLE: &str = "Extract base class to variable";
    pub const EXTRACT_DEFAULT_EXPORT_TO_VARIABLE: &str = "Extract default export to variable";
    pub const EXTRACT_BINDING_EXPRESSIONS_TO_VARIABLE: &str =
        "Extract binding expressions to variable";
    pub const ADD_ALL_MISSING_TYPE_ANNOTATIONS: &str = "Add all missing type annotations";
    pub const ADD_SATISFIES_AND_AN_INLINE_TYPE_ASSERTION_WITH: &str =
        "Add satisfies and an inline type assertion with '{0}'";
    pub const EXTRACT_TO_VARIABLE_AND_REPLACE_WITH_AS_TYPEOF: &str =
        "Extract to variable and replace with '{0} as typeof {0}'";
    pub const MARK_ARRAY_LITERAL_AS_CONST: &str = "Mark array literal as const";
    pub const ANNOTATE_TYPES_OF_PROPERTIES_EXPANDO_FUNCTION_IN_A_NAMESPACE: &str =
        "Annotate types of properties expando function in a namespace";
    pub const CONVERT_FUNCTION_TO_AN_ES2015_CLASS: &str = "Convert function to an ES2015 class";
    pub const CONVERT_TO_IN: &str = "Convert '{0}' to '{1} in {0}'";
    pub const EXTRACT_TO_IN: &str = "Extract to {0} in {1}";
    pub const EXTRACT_FUNCTION: &str = "Extract function";
    pub const EXTRACT_CONSTANT: &str = "Extract constant";
    pub const EXTRACT_TO_IN_ENCLOSING_SCOPE: &str = "Extract to {0} in enclosing scope";
    pub const EXTRACT_TO_IN_SCOPE: &str = "Extract to {0} in {1} scope";
    pub const ANNOTATE_WITH_TYPE_FROM_JSDOC: &str = "Annotate with type from JSDoc";
    pub const INFER_TYPE_OF_FROM_USAGE: &str = "Infer type of '{0}' from usage";
    pub const INFER_PARAMETER_TYPES_FROM_USAGE: &str = "Infer parameter types from usage";
    pub const CONVERT_TO_DEFAULT_IMPORT: &str = "Convert to default import";
    pub const INSTALL: &str = "Install '{0}'";
    pub const REPLACE_IMPORT_WITH: &str = "Replace import with '{0}'.";
    pub const USE_SYNTHETIC_DEFAULT_MEMBER: &str = "Use synthetic 'default' member.";
    pub const CONVERT_TO_ES_MODULE: &str = "Convert to ES module";
    pub const ADD_UNDEFINED_TYPE_TO_PROPERTY: &str = "Add 'undefined' type to property '{0}'";
    pub const ADD_INITIALIZER_TO_PROPERTY: &str = "Add initializer to property '{0}'";
    pub const ADD_DEFINITE_ASSIGNMENT_ASSERTION_TO_PROPERTY: &str =
        "Add definite assignment assertion to property '{0}'";
    pub const CONVERT_ALL_TYPE_LITERALS_TO_MAPPED_TYPE: &str =
        "Convert all type literals to mapped type";
    pub const ADD_ALL_MISSING_MEMBERS: &str = "Add all missing members";
    pub const INFER_ALL_TYPES_FROM_USAGE: &str = "Infer all types from usage";
    pub const DELETE_ALL_UNUSED_DECLARATIONS: &str = "Delete all unused declarations";
    pub const PREFIX_ALL_UNUSED_DECLARATIONS_WITH_WHERE_POSSIBLE: &str =
        "Prefix all unused declarations with '_' where possible";
    pub const FIX_ALL_DETECTED_SPELLING_ERRORS: &str = "Fix all detected spelling errors";
    pub const ADD_INITIALIZERS_TO_ALL_UNINITIALIZED_PROPERTIES: &str =
        "Add initializers to all uninitialized properties";
    pub const ADD_DEFINITE_ASSIGNMENT_ASSERTIONS_TO_ALL_UNINITIALIZED_PROPERTIES: &str =
        "Add definite assignment assertions to all uninitialized properties";
    pub const ADD_UNDEFINED_TYPE_TO_ALL_UNINITIALIZED_PROPERTIES: &str =
        "Add undefined type to all uninitialized properties";
    pub const CHANGE_ALL_JSDOC_STYLE_TYPES_TO_TYPESCRIPT: &str =
        "Change all jsdoc-style types to TypeScript";
    pub const CHANGE_ALL_JSDOC_STYLE_TYPES_TO_TYPESCRIPT_AND_ADD_UNDEFINED_TO_NULLABLE_TYPES: &str =
        "Change all jsdoc-style types to TypeScript (and add '| undefined' to nullable types)";
    pub const IMPLEMENT_ALL_UNIMPLEMENTED_INTERFACES: &str =
        "Implement all unimplemented interfaces";
    pub const INSTALL_ALL_MISSING_TYPES_PACKAGES: &str = "Install all missing types packages";
    pub const REWRITE_ALL_AS_INDEXED_ACCESS_TYPES: &str = "Rewrite all as indexed access types";
    pub const CONVERT_ALL_TO_DEFAULT_IMPORTS: &str = "Convert all to default imports";
    pub const MAKE_ALL_SUPER_CALLS_THE_FIRST_STATEMENT_IN_THEIR_CONSTRUCTOR: &str =
        "Make all 'super()' calls the first statement in their constructor";
    pub const ADD_QUALIFIER_TO_ALL_UNRESOLVED_VARIABLES_MATCHING_A_MEMBER_NAME: &str =
        "Add qualifier to all unresolved variables matching a member name";
    pub const CHANGE_ALL_EXTENDED_INTERFACES_TO_IMPLEMENTS: &str =
        "Change all extended interfaces to 'implements'";
    pub const ADD_ALL_MISSING_SUPER_CALLS: &str = "Add all missing super calls";
    pub const IMPLEMENT_ALL_INHERITED_ABSTRACT_CLASSES: &str =
        "Implement all inherited abstract classes";
    pub const ADD_ALL_MISSING_ASYNC_MODIFIERS: &str = "Add all missing 'async' modifiers";
    pub const ADD_TS_IGNORE_TO_ALL_ERROR_MESSAGES: &str = "Add '@ts-ignore' to all error messages";
    pub const ANNOTATE_EVERYTHING_WITH_TYPES_FROM_JSDOC: &str =
        "Annotate everything with types from JSDoc";
    pub const ADD_TO_ALL_UNCALLED_DECORATORS: &str = "Add '()' to all uncalled decorators";
    pub const CONVERT_ALL_CONSTRUCTOR_FUNCTIONS_TO_CLASSES: &str =
        "Convert all constructor functions to classes";
    pub const GENERATE_GET_AND_SET_ACCESSORS: &str = "Generate 'get' and 'set' accessors";
    pub const CONVERT_REQUIRE_TO_IMPORT: &str = "Convert 'require' to 'import'";
    pub const CONVERT_ALL_REQUIRE_TO_IMPORT: &str = "Convert all 'require' to 'import'";
    pub const MOVE_TO_A_NEW_FILE: &str = "Move to a new file";
    pub const REMOVE_UNREACHABLE_CODE: &str = "Remove unreachable code";
    pub const REMOVE_ALL_UNREACHABLE_CODE: &str = "Remove all unreachable code";
    pub const ADD_MISSING_TYPEOF: &str = "Add missing 'typeof'";
    pub const REMOVE_UNUSED_LABEL: &str = "Remove unused label";
    pub const REMOVE_ALL_UNUSED_LABELS: &str = "Remove all unused labels";
    pub const CONVERT_TO_MAPPED_OBJECT_TYPE: &str = "Convert '{0}' to mapped object type";
    pub const CONVERT_NAMESPACE_IMPORT_TO_NAMED_IMPORTS: &str =
        "Convert namespace import to named imports";
    pub const CONVERT_NAMED_IMPORTS_TO_NAMESPACE_IMPORT: &str =
        "Convert named imports to namespace import";
    pub const ADD_OR_REMOVE_BRACES_IN_AN_ARROW_FUNCTION: &str =
        "Add or remove braces in an arrow function";
    pub const ADD_BRACES_TO_ARROW_FUNCTION: &str = "Add braces to arrow function";
    pub const REMOVE_BRACES_FROM_ARROW_FUNCTION: &str = "Remove braces from arrow function";
    pub const CONVERT_DEFAULT_EXPORT_TO_NAMED_EXPORT: &str =
        "Convert default export to named export";
    pub const CONVERT_NAMED_EXPORT_TO_DEFAULT_EXPORT: &str =
        "Convert named export to default export";
    pub const ADD_MISSING_ENUM_MEMBER: &str = "Add missing enum member '{0}'";
    pub const ADD_ALL_MISSING_IMPORTS: &str = "Add all missing imports";
    pub const CONVERT_TO_ASYNC_FUNCTION: &str = "Convert to async function";
    pub const CONVERT_ALL_TO_ASYNC_FUNCTIONS: &str = "Convert all to async functions";
    pub const ADD_MISSING_CALL_PARENTHESES: &str = "Add missing call parentheses";
    pub const ADD_ALL_MISSING_CALL_PARENTHESES: &str = "Add all missing call parentheses";
    pub const ADD_UNKNOWN_CONVERSION_FOR_NON_OVERLAPPING_TYPES: &str =
        "Add 'unknown' conversion for non-overlapping types";
    pub const ADD_UNKNOWN_TO_ALL_CONVERSIONS_OF_NON_OVERLAPPING_TYPES: &str =
        "Add 'unknown' to all conversions of non-overlapping types";
    pub const ADD_MISSING_NEW_OPERATOR_TO_CALL: &str = "Add missing 'new' operator to call";
    pub const ADD_MISSING_NEW_OPERATOR_TO_ALL_CALLS: &str =
        "Add missing 'new' operator to all calls";
    pub const ADD_NAMES_TO_ALL_PARAMETERS_WITHOUT_NAMES: &str =
        "Add names to all parameters without names";
    pub const ENABLE_THE_EXPERIMENTALDECORATORS_OPTION_IN_YOUR_CONFIGURATION_FILE: &str =
        "Enable the 'experimentalDecorators' option in your configuration file";
    pub const CONVERT_PARAMETERS_TO_DESTRUCTURED_OBJECT: &str =
        "Convert parameters to destructured object";
    pub const EXTRACT_TYPE: &str = "Extract type";
    pub const EXTRACT_TO_TYPE_ALIAS: &str = "Extract to type alias";
    pub const EXTRACT_TO_TYPEDEF: &str = "Extract to typedef";
    pub const INFER_THIS_TYPE_OF_FROM_USAGE: &str = "Infer 'this' type of '{0}' from usage";
    pub const ADD_CONST_TO_UNRESOLVED_VARIABLE: &str = "Add 'const' to unresolved variable";
    pub const ADD_CONST_TO_ALL_UNRESOLVED_VARIABLES: &str =
        "Add 'const' to all unresolved variables";
    pub const ADD_AWAIT: &str = "Add 'await'";
    pub const ADD_AWAIT_TO_INITIALIZER_FOR: &str = "Add 'await' to initializer for '{0}'";
    pub const FIX_ALL_EXPRESSIONS_POSSIBLY_MISSING_AWAIT: &str =
        "Fix all expressions possibly missing 'await'";
    pub const REMOVE_UNNECESSARY_AWAIT: &str = "Remove unnecessary 'await'";
    pub const REMOVE_ALL_UNNECESSARY_USES_OF_AWAIT: &str = "Remove all unnecessary uses of 'await'";
    pub const ENABLE_THE_JSX_FLAG_IN_YOUR_CONFIGURATION_FILE: &str =
        "Enable the '--jsx' flag in your configuration file";
    pub const ADD_AWAIT_TO_INITIALIZERS: &str = "Add 'await' to initializers";
    pub const EXTRACT_TO_INTERFACE: &str = "Extract to interface";
    pub const CONVERT_TO_A_BIGINT_NUMERIC_LITERAL: &str = "Convert to a bigint numeric literal";
    pub const CONVERT_ALL_TO_BIGINT_NUMERIC_LITERALS: &str =
        "Convert all to bigint numeric literals";
    pub const CONVERT_CONST_TO_LET: &str = "Convert 'const' to 'let'";
    pub const PREFIX_WITH_DECLARE: &str = "Prefix with 'declare'";
    pub const PREFIX_ALL_INCORRECT_PROPERTY_DECLARATIONS_WITH_DECLARE: &str =
        "Prefix all incorrect property declarations with 'declare'";
    pub const CONVERT_TO_TEMPLATE_STRING: &str = "Convert to template string";
    pub const ADD_EXPORT_TO_MAKE_THIS_FILE_INTO_A_MODULE: &str =
        "Add 'export {}' to make this file into a module";
    pub const SET_THE_TARGET_OPTION_IN_YOUR_CONFIGURATION_FILE_TO: &str =
        "Set the 'target' option in your configuration file to '{0}'";
    pub const SET_THE_MODULE_OPTION_IN_YOUR_CONFIGURATION_FILE_TO: &str =
        "Set the 'module' option in your configuration file to '{0}'";
    pub const CONVERT_INVALID_CHARACTER_TO_ITS_HTML_ENTITY_CODE: &str =
        "Convert invalid character to its html entity code";
    pub const CONVERT_ALL_INVALID_CHARACTERS_TO_HTML_ENTITY_CODE: &str =
        "Convert all invalid characters to HTML entity code";
    pub const CONVERT_ALL_CONST_TO_LET: &str = "Convert all 'const' to 'let'";
    pub const CONVERT_FUNCTION_EXPRESSION_TO_ARROW_FUNCTION: &str =
        "Convert function expression '{0}' to arrow function";
    pub const CONVERT_FUNCTION_DECLARATION_TO_ARROW_FUNCTION: &str =
        "Convert function declaration '{0}' to arrow function";
    pub const FIX_ALL_IMPLICIT_THIS_ERRORS: &str = "Fix all implicit-'this' errors";
    pub const WRAP_INVALID_CHARACTER_IN_AN_EXPRESSION_CONTAINER: &str =
        "Wrap invalid character in an expression container";
    pub const WRAP_ALL_INVALID_CHARACTERS_IN_AN_EXPRESSION_CONTAINER: &str =
        "Wrap all invalid characters in an expression container";
    pub const VISIT_HTTPS_AKA_MS_TSCONFIG_TO_READ_MORE_ABOUT_THIS_FILE: &str =
        "Visit https://aka.ms/tsconfig to read more about this file";
    pub const ADD_A_RETURN_STATEMENT: &str = "Add a return statement";
    pub const REMOVE_BRACES_FROM_ARROW_FUNCTION_BODY: &str =
        "Remove braces from arrow function body";
    pub const WRAP_THE_FOLLOWING_BODY_WITH_PARENTHESES_WHICH_SHOULD_BE_AN_OBJECT_LITERAL: &str =
        "Wrap the following body with parentheses which should be an object literal";
    pub const ADD_ALL_MISSING_RETURN_STATEMENT: &str = "Add all missing return statement";
    pub const REMOVE_BRACES_FROM_ALL_ARROW_FUNCTION_BODIES_WITH_RELEVANT_ISSUES: &str =
        "Remove braces from all arrow function bodies with relevant issues";
    pub const WRAP_ALL_OBJECT_LITERAL_WITH_PARENTHESES: &str =
        "Wrap all object literal with parentheses";
    pub const MOVE_LABELED_TUPLE_ELEMENT_MODIFIERS_TO_LABELS: &str =
        "Move labeled tuple element modifiers to labels";
    pub const CONVERT_OVERLOAD_LIST_TO_SINGLE_SIGNATURE: &str =
        "Convert overload list to single signature";
    pub const GENERATE_GET_AND_SET_ACCESSORS_FOR_ALL_OVERRIDING_PROPERTIES: &str =
        "Generate 'get' and 'set' accessors for all overriding properties";
    pub const WRAP_IN_JSX_FRAGMENT: &str = "Wrap in JSX fragment";
    pub const WRAP_ALL_UNPARENTED_JSX_IN_JSX_FRAGMENT: &str =
        "Wrap all unparented JSX in JSX fragment";
    pub const CONVERT_ARROW_FUNCTION_OR_FUNCTION_EXPRESSION: &str =
        "Convert arrow function or function expression";
    pub const CONVERT_TO_ANONYMOUS_FUNCTION: &str = "Convert to anonymous function";
    pub const CONVERT_TO_NAMED_FUNCTION: &str = "Convert to named function";
    pub const CONVERT_TO_ARROW_FUNCTION: &str = "Convert to arrow function";
    pub const REMOVE_PARENTHESES: &str = "Remove parentheses";
    pub const COULD_NOT_FIND_A_CONTAINING_ARROW_FUNCTION: &str =
        "Could not find a containing arrow function";
    pub const CONTAINING_FUNCTION_IS_NOT_AN_ARROW_FUNCTION: &str =
        "Containing function is not an arrow function";
    pub const COULD_NOT_FIND_EXPORT_STATEMENT: &str = "Could not find export statement";
    pub const THIS_FILE_ALREADY_HAS_A_DEFAULT_EXPORT: &str =
        "This file already has a default export";
    pub const COULD_NOT_FIND_IMPORT_CLAUSE: &str = "Could not find import clause";
    pub const COULD_NOT_FIND_NAMESPACE_IMPORT_OR_NAMED_IMPORTS: &str =
        "Could not find namespace import or named imports";
    pub const SELECTION_IS_NOT_A_VALID_TYPE_NODE: &str = "Selection is not a valid type node";
    pub const NO_TYPE_COULD_BE_EXTRACTED_FROM_THIS_TYPE_NODE: &str =
        "No type could be extracted from this type node";
    pub const COULD_NOT_FIND_PROPERTY_FOR_WHICH_TO_GENERATE_ACCESSOR: &str =
        "Could not find property for which to generate accessor";
    pub const NAME_IS_NOT_VALID: &str = "Name is not valid";
    pub const CAN_ONLY_CONVERT_PROPERTY_WITH_MODIFIER: &str =
        "Can only convert property with modifier";
    pub const SWITCH_EACH_MISUSED_TO: &str = "Switch each misused '{0}' to '{1}'";
    pub const CONVERT_TO_OPTIONAL_CHAIN_EXPRESSION: &str = "Convert to optional chain expression";
    pub const COULD_NOT_FIND_CONVERTIBLE_ACCESS_EXPRESSION: &str =
        "Could not find convertible access expression";
    pub const COULD_NOT_FIND_MATCHING_ACCESS_EXPRESSIONS: &str =
        "Could not find matching access expressions";
    pub const CAN_ONLY_CONVERT_LOGICAL_AND_ACCESS_CHAINS: &str =
        "Can only convert logical AND access chains";
    pub const ADD_VOID_TO_PROMISE_RESOLVED_WITHOUT_A_VALUE: &str =
        "Add 'void' to Promise resolved without a value";
    pub const ADD_VOID_TO_ALL_PROMISES_RESOLVED_WITHOUT_A_VALUE: &str =
        "Add 'void' to all Promises resolved without a value";
    pub const USE_ELEMENT_ACCESS_FOR: &str = "Use element access for '{0}'";
    pub const USE_ELEMENT_ACCESS_FOR_ALL_UNDECLARED_PROPERTIES: &str =
        "Use element access for all undeclared properties.";
    pub const DELETE_ALL_UNUSED_IMPORTS: &str = "Delete all unused imports";
    pub const INFER_FUNCTION_RETURN_TYPE: &str = "Infer function return type";
    pub const RETURN_TYPE_MUST_BE_INFERRED_FROM_A_FUNCTION: &str =
        "Return type must be inferred from a function";
    pub const COULD_NOT_DETERMINE_FUNCTION_RETURN_TYPE: &str =
        "Could not determine function return type";
    pub const COULD_NOT_CONVERT_TO_ARROW_FUNCTION: &str = "Could not convert to arrow function";
    pub const COULD_NOT_CONVERT_TO_NAMED_FUNCTION: &str = "Could not convert to named function";
    pub const COULD_NOT_CONVERT_TO_ANONYMOUS_FUNCTION: &str =
        "Could not convert to anonymous function";
    pub const CAN_ONLY_CONVERT_STRING_CONCATENATIONS_AND_STRING_LITERALS: &str =
        "Can only convert string concatenations and string literals";
    pub const SELECTION_IS_NOT_A_VALID_STATEMENT_OR_STATEMENTS: &str =
        "Selection is not a valid statement or statements";
    pub const ADD_MISSING_FUNCTION_DECLARATION: &str = "Add missing function declaration '{0}'";
    pub const ADD_ALL_MISSING_FUNCTION_DECLARATIONS: &str = "Add all missing function declarations";
    pub const METHOD_NOT_IMPLEMENTED: &str = "Method not implemented.";
    pub const FUNCTION_NOT_IMPLEMENTED: &str = "Function not implemented.";
    pub const ADD_OVERRIDE_MODIFIER: &str = "Add 'override' modifier";
    pub const REMOVE_OVERRIDE_MODIFIER: &str = "Remove 'override' modifier";
    pub const ADD_ALL_MISSING_OVERRIDE_MODIFIERS: &str = "Add all missing 'override' modifiers";
    pub const REMOVE_ALL_UNNECESSARY_OVERRIDE_MODIFIERS: &str =
        "Remove all unnecessary 'override' modifiers";
    pub const CAN_ONLY_CONVERT_NAMED_EXPORT: &str = "Can only convert named export";
    pub const ADD_MISSING_PROPERTIES: &str = "Add missing properties";
    pub const ADD_ALL_MISSING_PROPERTIES: &str = "Add all missing properties";
    pub const ADD_MISSING_ATTRIBUTES: &str = "Add missing attributes";
    pub const ADD_ALL_MISSING_ATTRIBUTES: &str = "Add all missing attributes";
    pub const ADD_UNDEFINED_TO_OPTIONAL_PROPERTY_TYPE: &str =
        "Add 'undefined' to optional property type";
    pub const CONVERT_NAMED_IMPORTS_TO_DEFAULT_IMPORT: &str =
        "Convert named imports to default import";
    pub const DELETE_UNUSED_PARAM_TAG: &str = "Delete unused '@param' tag '{0}'";
    pub const DELETE_ALL_UNUSED_PARAM_TAGS: &str = "Delete all unused '@param' tags";
    pub const RENAME_PARAM_TAG_NAME_TO: &str = "Rename '@param' tag name '{0}' to '{1}'";
    pub const USE: &str = "Use `{0}`.";
    pub const USE_NUMBER_ISNAN_IN_ALL_CONDITIONS: &str = "Use `Number.isNaN` in all conditions.";
    pub const CONVERT_TYPEDEF_TO_TYPESCRIPT_TYPE: &str = "Convert typedef to TypeScript type.";
    pub const CONVERT_ALL_TYPEDEF_TO_TYPESCRIPT_TYPES: &str =
        "Convert all typedef to TypeScript types.";
    pub const MOVE_TO_FILE: &str = "Move to file";
    pub const CANNOT_MOVE_TO_FILE_SELECTED_FILE_IS_INVALID: &str =
        "Cannot move to file, selected file is invalid";
    pub const USE_IMPORT_TYPE: &str = "Use 'import type'";
    pub const USE_TYPE: &str = "Use 'type {0}'";
    pub const FIX_ALL_WITH_TYPE_ONLY_IMPORTS: &str = "Fix all with type-only imports";
    pub const CANNOT_MOVE_STATEMENTS_TO_THE_SELECTED_FILE: &str =
        "Cannot move statements to the selected file";
    pub const INLINE_VARIABLE: &str = "Inline variable";
    pub const COULD_NOT_FIND_VARIABLE_TO_INLINE: &str = "Could not find variable to inline.";
    pub const VARIABLES_WITH_MULTIPLE_DECLARATIONS_CANNOT_BE_INLINED: &str =
        "Variables with multiple declarations cannot be inlined.";
    pub const ADD_MISSING_COMMA_FOR_OBJECT_MEMBER_COMPLETION: &str =
        "Add missing comma for object member completion '{0}'.";
    pub const ADD_MISSING_PARAMETER_TO: &str = "Add missing parameter to '{0}'";
    pub const ADD_MISSING_PARAMETERS_TO: &str = "Add missing parameters to '{0}'";
    pub const ADD_ALL_MISSING_PARAMETERS: &str = "Add all missing parameters";
    pub const ADD_OPTIONAL_PARAMETER_TO: &str = "Add optional parameter to '{0}'";
    pub const ADD_OPTIONAL_PARAMETERS_TO: &str = "Add optional parameters to '{0}'";
    pub const ADD_ALL_OPTIONAL_PARAMETERS: &str = "Add all optional parameters";
    pub const WRAP_IN_PARENTHESES: &str = "Wrap in parentheses";
    pub const WRAP_ALL_INVALID_DECORATOR_EXPRESSIONS_IN_PARENTHESES: &str =
        "Wrap all invalid decorator expressions in parentheses";
    pub const ADD_RESOLUTION_MODE_IMPORT_ATTRIBUTE: &str = "Add 'resolution-mode' import attribute";
    pub const ADD_RESOLUTION_MODE_IMPORT_ATTRIBUTE_TO_ALL_TYPE_ONLY_IMPORTS_THAT_NEED_IT: &str =
        "Add 'resolution-mode' import attribute to all type-only imports that need it";
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's diagnosticMessages.json
pub mod diagnostic_codes {
    pub const IMPORT_EXPECTS_FROM_CLAUSE: u32 = 2000;
    pub const UNTERMINATED_STRING_LITERAL: u32 = 1002;
    pub const IDENTIFIER_EXPECTED: u32 = 1003;
    pub const EXPECTED: u32 = 1005;
    pub const A_FILE_CANNOT_HAVE_A_REFERENCE_TO_ITSELF: u32 = 1006;
    pub const THE_PARSER_EXPECTED_TO_FIND_A_TO_MATCH_THE_TOKEN_HERE: u32 = 1007;
    pub const TRAILING_COMMA_NOT_ALLOWED: u32 = 1009;
    pub const EXPECTED_2: u32 = 1010;
    pub const AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT: u32 = 1011;
    pub const UNEXPECTED_TOKEN: u32 = 1012;
    pub const A_REST_PARAMETER_OR_BINDING_PATTERN_MAY_NOT_HAVE_A_TRAILING_COMMA: u32 = 1013;
    pub const A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST: u32 = 1014;
    pub const PARAMETER_CANNOT_HAVE_QUESTION_MARK_AND_INITIALIZER: u32 = 1015;
    pub const A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER: u32 = 1016;
    pub const AN_INDEX_SIGNATURE_CANNOT_HAVE_A_REST_PARAMETER: u32 = 1017;
    pub const AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER: u32 = 1018;
    pub const AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_A_QUESTION_MARK: u32 = 1019;
    pub const AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER: u32 = 1020;
    pub const AN_INDEX_SIGNATURE_MUST_HAVE_A_TYPE_ANNOTATION: u32 = 1021;
    pub const AN_INDEX_SIGNATURE_PARAMETER_MUST_HAVE_A_TYPE_ANNOTATION: u32 = 1022;
    pub const READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE: u32 =
        1024;
    pub const AN_INDEX_SIGNATURE_CANNOT_HAVE_A_TRAILING_COMMA: u32 = 1025;
    pub const ACCESSIBILITY_MODIFIER_ALREADY_SEEN: u32 = 1028;
    pub const MODIFIER_MUST_PRECEDE_MODIFIER: u32 = 1029;
    pub const MODIFIER_ALREADY_SEEN: u32 = 1030;
    pub const MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND: u32 = 1031;
    pub const SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS: u32 = 1034;
    pub const ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES: u32 = 1035;
    pub const STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: u32 = 1036;
    pub const A_DECLARE_MODIFIER_CANNOT_BE_USED_IN_AN_ALREADY_AMBIENT_CONTEXT: u32 = 1038;
    pub const INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: u32 = 1039;
    pub const MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT: u32 = 1040;
    pub const MODIFIER_CANNOT_BE_USED_HERE: u32 = 1042;
    pub const MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT: u32 = 1044;
    pub const TOP_LEVEL_DECLARATIONS_IN_D_TS_FILES_MUST_START_WITH_EITHER_A_DECLARE_OR_EXPORT: u32 =
        1046;
    pub const A_REST_PARAMETER_CANNOT_BE_OPTIONAL: u32 = 1047;
    pub const A_REST_PARAMETER_CANNOT_HAVE_AN_INITIALIZER: u32 = 1048;
    pub const A_SET_ACCESSOR_MUST_HAVE_EXACTLY_ONE_PARAMETER: u32 = 1049;
    pub const A_SET_ACCESSOR_CANNOT_HAVE_AN_OPTIONAL_PARAMETER: u32 = 1051;
    pub const A_SET_ACCESSOR_PARAMETER_CANNOT_HAVE_AN_INITIALIZER: u32 = 1052;
    pub const A_SET_ACCESSOR_CANNOT_HAVE_REST_PARAMETER: u32 = 1053;
    pub const A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS: u32 = 1054;
    pub const TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER: u32 =
        1055;
    pub const ACCESSORS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_5_AND_HIGHER: u32 = 1056;
    pub const THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT: u32 =
        1058;
    pub const A_PROMISE_MUST_HAVE_A_THEN_METHOD: u32 = 1059;
    pub const THE_FIRST_PARAMETER_OF_THE_THEN_METHOD_OF_A_PROMISE_MUST_BE_A_CALLBACK: u32 = 1060;
    pub const ENUM_MEMBER_MUST_HAVE_INITIALIZER: u32 = 1061;
    pub const TYPE_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_THE_FULFILLMENT_CALLBACK_OF_ITS_OWN:
        u32 = 1062;
    pub const AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE: u32 = 1063;
    pub const THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE:
        u32 = 1064;
    pub const THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE_2:
        u32 = 1065;
    pub const IN_AMBIENT_ENUM_DECLARATIONS_MEMBER_INITIALIZER_MUST_BE_CONSTANT_EXPRESSION: u32 =
        1066;
    pub const UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED: u32 = 1068;
    pub const UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES: u32 = 1069;
    pub const MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER: u32 = 1070;
    pub const MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE: u32 = 1071;
    pub const A_MODIFIER_CANNOT_BE_USED_WITH_AN_IMPORT_DECLARATION: u32 = 1079;
    pub const INVALID_REFERENCE_DIRECTIVE_SYNTAX: u32 = 1084;
    pub const MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION: u32 = 1089;
    pub const MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER: u32 = 1090;
    pub const ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_IN_STATEMENT: u32 = 1091;
    pub const TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION: u32 = 1092;
    pub const TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION: u32 = 1093;
    pub const AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS: u32 = 1094;
    pub const A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION: u32 = 1095;
    pub const AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER: u32 = 1096;
    pub const LIST_CANNOT_BE_EMPTY: u32 = 1097;
    pub const TYPE_PARAMETER_LIST_CANNOT_BE_EMPTY: u32 = 1098;
    pub const TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY: u32 = 1099;
    pub const INVALID_USE_OF_IN_STRICT_MODE: u32 = 1100;
    pub const WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE: u32 = 1101;
    pub const DELETE_CANNOT_BE_CALLED_ON_AN_IDENTIFIER_IN_STRICT_MODE: u32 = 1102;
    pub const FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF:
        u32 = 1103;
    pub const A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT: u32 =
        1104;
    pub const A_BREAK_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_OR_SWITCH_STATE:
        u32 = 1105;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_ASYNC: u32 = 1106;
    pub const JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY: u32 = 1107;
    pub const A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY: u32 = 1108;
    pub const EXPRESSION_EXPECTED: u32 = 1109;
    pub const TYPE_EXPECTED: u32 = 1110;
    pub const PRIVATE_FIELD_MUST_BE_DECLARED_IN_AN_ENCLOSING_CLASS: u32 = 1111;
    pub const A_DEFAULT_CLAUSE_CANNOT_APPEAR_MORE_THAN_ONCE_IN_A_SWITCH_STATEMENT: u32 = 1113;
    pub const DUPLICATE_LABEL: u32 = 1114;
    pub const A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMEN:
        u32 = 1115;
    pub const A_BREAK_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_STATEMENT: u32 = 1116;
    pub const AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME: u32 = 1117;
    pub const AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_GET_SET_ACCESSORS_WITH_THE_SAME_NAME: u32 =
        1118;
    pub const AN_OBJECT_LITERAL_CANNOT_HAVE_PROPERTY_AND_ACCESSOR_WITH_THE_SAME_NAME: u32 = 1119;
    pub const AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS: u32 = 1120;
    pub const OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX: u32 = 1121;
    pub const VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY: u32 = 1123;
    pub const DIGIT_EXPECTED: u32 = 1124;
    pub const HEXADECIMAL_DIGIT_EXPECTED: u32 = 1125;
    pub const UNEXPECTED_END_OF_TEXT: u32 = 1126;
    pub const INVALID_CHARACTER: u32 = 1127;
    pub const DECLARATION_OR_STATEMENT_EXPECTED: u32 = 1128;
    pub const STATEMENT_EXPECTED: u32 = 1129;
    pub const CASE_OR_DEFAULT_EXPECTED: u32 = 1130;
    pub const PROPERTY_OR_SIGNATURE_EXPECTED: u32 = 1131;
    pub const ENUM_MEMBER_EXPECTED: u32 = 1132;
    pub const VARIABLE_DECLARATION_EXPECTED: u32 = 1134;
    pub const ARGUMENT_EXPRESSION_EXPECTED: u32 = 1135;
    pub const PROPERTY_ASSIGNMENT_EXPECTED: u32 = 1136;
    pub const EXPRESSION_OR_COMMA_EXPECTED: u32 = 1137;
    pub const PARAMETER_DECLARATION_EXPECTED: u32 = 1138;
    pub const TYPE_PARAMETER_DECLARATION_EXPECTED: u32 = 1139;
    pub const TYPE_ARGUMENT_EXPECTED: u32 = 1140;
    pub const STRING_LITERAL_EXPECTED: u32 = 1141;
    pub const LINE_BREAK_NOT_PERMITTED_HERE: u32 = 1142;
    pub const OR_EXPECTED: u32 = 1144;
    pub const OR_JSX_ELEMENT_EXPECTED: u32 = 1145;
    pub const DECLARATION_EXPECTED: u32 = 1146;
    pub const IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE: u32 = 1147;
    pub const CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE: u32 = 1148;
    pub const FILE_NAME_DIFFERS_FROM_ALREADY_INCLUDED_FILE_NAME_ONLY_IN_CASING: u32 = 1149;
    pub const DECLARATIONS_MUST_BE_INITIALIZED: u32 = 1155;
    pub const DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK: u32 = 1156;
    pub const UNTERMINATED_TEMPLATE_LITERAL: u32 = 1160;
    pub const UNTERMINATED_REGULAR_EXPRESSION_LITERAL: u32 = 1161;
    pub const AN_OBJECT_MEMBER_CANNOT_BE_DECLARED_OPTIONAL: u32 = 1162;
    pub const A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY: u32 = 1163;
    pub const COMPUTED_PROPERTY_NAMES_ARE_NOT_ALLOWED_IN_ENUMS: u32 = 1164;
    pub const A_COMPUTED_PROPERTY_NAME_IN_AN_AMBIENT_CONTEXT_MUST_REFER_TO_AN_EXPRESSION_WHOSE:
        u32 = 1165;
    pub const A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE:
        u32 = 1166;
    pub const A_COMPUTED_PROPERTY_NAME_IN_A_METHOD_OVERLOAD_MUST_REFER_TO_AN_EXPRESSION_WHOSE: u32 =
        1168;
    pub const A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE: u32 =
        1169;
    pub const A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP:
        u32 = 1170;
    pub const A_COMMA_EXPRESSION_IS_NOT_ALLOWED_IN_A_COMPUTED_PROPERTY_NAME: u32 = 1171;
    pub const EXTENDS_CLAUSE_ALREADY_SEEN: u32 = 1172;
    pub const EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE: u32 = 1173;
    pub const CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS: u32 = 1174;
    pub const IMPLEMENTS_CLAUSE_ALREADY_SEEN: u32 = 1175;
    pub const INTERFACE_DECLARATION_CANNOT_HAVE_IMPLEMENTS_CLAUSE: u32 = 1176;
    pub const BINARY_DIGIT_EXPECTED: u32 = 1177;
    pub const OCTAL_DIGIT_EXPECTED: u32 = 1178;
    pub const UNEXPECTED_TOKEN_EXPECTED: u32 = 1179;
    pub const PROPERTY_DESTRUCTURING_PATTERN_EXPECTED: u32 = 1180;
    pub const ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED: u32 = 1181;
    pub const A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER: u32 = 1182;
    pub const AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS: u32 = 1183;
    pub const MODIFIERS_CANNOT_APPEAR_HERE: u32 = 1184;
    pub const MERGE_CONFLICT_MARKER_ENCOUNTERED: u32 = 1185;
    pub const A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER: u32 = 1186;
    pub const A_PARAMETER_PROPERTY_MAY_NOT_BE_DECLARED_USING_A_BINDING_PATTERN: u32 = 1187;
    pub const ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_OF_STATEMENT: u32 = 1188;
    pub const THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER: u32 = 1189;
    pub const THE_VARIABLE_DECLARATION_OF_A_FOR_OF_STATEMENT_CANNOT_HAVE_AN_INITIALIZER: u32 = 1190;
    pub const AN_IMPORT_DECLARATION_CANNOT_HAVE_MODIFIERS: u32 = 1191;
    pub const MODULE_HAS_NO_DEFAULT_EXPORT: u32 = 1192;
    pub const AN_EXPORT_DECLARATION_CANNOT_HAVE_MODIFIERS: u32 = 1193;
    pub const EXPORT_DECLARATIONS_ARE_NOT_PERMITTED_IN_A_NAMESPACE: u32 = 1194;
    pub const EXPORT_DOES_NOT_RE_EXPORT_A_DEFAULT: u32 = 1195;
    pub const CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED: u32 = 1196;
    pub const CATCH_CLAUSE_VARIABLE_CANNOT_HAVE_AN_INITIALIZER: u32 = 1197;
    pub const AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE: u32 =
        1198;
    pub const UNTERMINATED_UNICODE_ESCAPE_SEQUENCE: u32 = 1199;
    pub const LINE_TERMINATOR_NOT_PERMITTED_BEFORE_ARROW: u32 = 1200;
    pub const IMPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN:
        u32 = 1202;
    pub const EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN:
        u32 = 1203;
    pub const RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE: u32 = 1205;
    pub const DECORATORS_ARE_NOT_VALID_HERE: u32 = 1206;
    pub const DECORATORS_CANNOT_BE_APPLIED_TO_MULTIPLE_GET_SET_ACCESSORS_OF_THE_SAME_NAME: u32 =
        1207;
    pub const INVALID_OPTIONAL_CHAIN_FROM_NEW_EXPRESSION_DID_YOU_MEAN_TO_CALL: u32 = 1209;
    pub const CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT:
        u32 = 1210;
    pub const A_CLASS_DECLARATION_WITHOUT_THE_DEFAULT_MODIFIER_MUST_HAVE_A_NAME: u32 = 1211;
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE: u32 = 1212;
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO:
        u32 = 1213;
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY: u32 =
        1214;
    pub const INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE: u32 = 1215;
    pub const IDENTIFIER_EXPECTED_ESMODULE_IS_RESERVED_AS_AN_EXPORTED_MARKER_WHEN_TRANSFORMING:
        u32 = 1216;
    pub const EXPORT_ASSIGNMENT_IS_NOT_SUPPORTED_WHEN_MODULE_FLAG_IS_SYSTEM: u32 = 1218;
    pub const GENERATORS_ARE_NOT_ALLOWED_IN_AN_AMBIENT_CONTEXT: u32 = 1221;
    pub const AN_OVERLOAD_SIGNATURE_CANNOT_BE_DECLARED_AS_A_GENERATOR: u32 = 1222;
    pub const TAG_ALREADY_SPECIFIED: u32 = 1223;
    pub const SIGNATURE_MUST_BE_A_TYPE_PREDICATE: u32 = 1224;
    pub const CANNOT_FIND_PARAMETER: u32 = 1225;
    pub const TYPE_PREDICATE_IS_NOT_ASSIGNABLE_TO: u32 = 1226;
    pub const PARAMETER_IS_NOT_IN_THE_SAME_POSITION_AS_PARAMETER: u32 = 1227;
    pub const A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO:
        u32 = 1228;
    pub const A_TYPE_PREDICATE_CANNOT_REFERENCE_A_REST_PARAMETER: u32 = 1229;
    pub const A_TYPE_PREDICATE_CANNOT_REFERENCE_ELEMENT_IN_A_BINDING_PATTERN: u32 = 1230;
    pub const AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION: u32 =
        1231;
    pub const AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE:
        u32 = 1232;
    pub const AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE:
        u32 = 1233;
    pub const AN_AMBIENT_MODULE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_IN_A_FILE: u32 = 1234;
    pub const A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODUL:
        u32 = 1235;
    pub const THE_RETURN_TYPE_OF_A_PROPERTY_DECORATOR_FUNCTION_MUST_BE_EITHER_VOID_OR_ANY: u32 =
        1236;
    pub const THE_RETURN_TYPE_OF_A_PARAMETER_DECORATOR_FUNCTION_MUST_BE_EITHER_VOID_OR_ANY: u32 =
        1237;
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION: u32 =
        1238;
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_PARAMETER_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION: u32 =
        1239;
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION: u32 =
        1240;
    pub const UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION: u32 =
        1241;
    pub const ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION: u32 =
        1242;
    pub const MODIFIER_CANNOT_BE_USED_WITH_MODIFIER: u32 = 1243;
    pub const ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS: u32 = 1244;
    pub const METHOD_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT: u32 = 1245;
    pub const AN_INTERFACE_PROPERTY_CANNOT_HAVE_AN_INITIALIZER: u32 = 1246;
    pub const A_TYPE_LITERAL_PROPERTY_CANNOT_HAVE_AN_INITIALIZER: u32 = 1247;
    pub const A_CLASS_MEMBER_CANNOT_HAVE_THE_KEYWORD: u32 = 1248;
    pub const A_DECORATOR_CAN_ONLY_DECORATE_A_METHOD_IMPLEMENTATION_NOT_AN_OVERLOAD: u32 = 1249;
    pub const FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN:
        u32 = 1250;
    pub const FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_2:
        u32 = 1251;
    pub const FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_3:
        u32 = 1252;
    pub const ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS: u32 = 1253;
    pub const A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR:
        u32 = 1254;
    pub const A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT: u32 = 1255;
    pub const A_REQUIRED_ELEMENT_CANNOT_FOLLOW_AN_OPTIONAL_ELEMENT: u32 = 1257;
    pub const A_DEFAULT_EXPORT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION: u32 = 1258;
    pub const MODULE_CAN_ONLY_BE_DEFAULT_IMPORTED_USING_THE_FLAG: u32 = 1259;
    pub const KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS: u32 = 1260;
    pub const ALREADY_INCLUDED_FILE_NAME_DIFFERS_FROM_FILE_NAME_ONLY_IN_CASING: u32 = 1261;
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE: u32 = 1262;
    pub const DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS: u32 =
        1263;
    pub const DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS:
        u32 = 1264;
    pub const A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT: u32 = 1265;
    pub const AN_OPTIONAL_ELEMENT_CANNOT_FOLLOW_A_REST_ELEMENT: u32 = 1266;
    pub const PROPERTY_CANNOT_HAVE_AN_INITIALIZER_BECAUSE_IT_IS_MARKED_ABSTRACT: u32 = 1267;
    pub const AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT:
        u32 = 1268;
    pub const CANNOT_USE_EXPORT_IMPORT_ON_A_TYPE_OR_TYPE_ONLY_NAMESPACE_WHEN_IS_ENABLED: u32 = 1269;
    pub const DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE: u32 = 1270;
    pub const DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY: u32 = 1271;
    pub const A_TYPE_REFERENCED_IN_A_DECORATED_SIGNATURE_MUST_BE_IMPORTED_WITH_IMPORT_TYPE_OR: u32 =
        1272;
    pub const MODIFIER_CANNOT_APPEAR_ON_A_TYPE_PARAMETER: u32 = 1273;
    pub const MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_CLASS_INTERFACE_OR_TYPE_ALIAS: u32 =
        1274;
    pub const ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION: u32 = 1275;
    pub const AN_ACCESSOR_PROPERTY_CANNOT_BE_DECLARED_OPTIONAL: u32 = 1276;
    pub const MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_FUNCTION_METHOD_OR_CLASS: u32 =
        1277;
    pub const THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS: u32 =
        1278;
    pub const THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS_A:
        u32 = 1279;
    pub const NAMESPACES_ARE_NOT_ALLOWED_IN_GLOBAL_SCRIPT_FILES_WHEN_IS_ENABLED_IF_THIS_FILE_I:
        u32 = 1280;
    pub const CANNOT_ACCESS_FROM_ANOTHER_FILE_WITHOUT_QUALIFICATION_WHEN_IS_ENABLED_USE_INSTEA:
        u32 = 1281;
    pub const AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE:
        u32 = 1282;
    pub const AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E:
        u32 = 1283;
    pub const AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU:
        u32 = 1284;
    pub const AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL:
        u32 = 1285;
    pub const ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT:
        u32 = 1286;
    pub const A_TOP_LEVEL_EXPORT_MODIFIER_CANNOT_BE_USED_ON_VALUE_DECLARATIONS_IN_A_COMMONJS_M:
        u32 = 1287;
    pub const AN_IMPORT_ALIAS_CANNOT_RESOLVE_TO_A_TYPE_OR_TYPE_ONLY_DECLARATION_WHEN_VERBATIMM:
        u32 = 1288;
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BE:
        u32 = 1289;
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BE_2:
        u32 = 1290;
    pub const RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING:
        u32 = 1291;
    pub const RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_2:
        u32 = 1292;
    pub const ECMASCRIPT_MODULE_SYNTAX_IS_NOT_ALLOWED_IN_A_COMMONJS_MODULE_WHEN_MODULE_IS_SET: u32 =
        1293;
    pub const THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED: u32 = 1294;
    pub const ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2:
        u32 = 1295;
    pub const WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_AN_ASYNC_FUNCTION_BLOCK: u32 = 1300;
    pub const AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS: u32 =
        1308;
    pub const THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_AND_CANNOT_USE_AWAIT_AT_THE_TOP_LEVEL: u32 =
        1309;
    pub const DID_YOU_MEAN_TO_USE_A_AN_CAN_ONLY_FOLLOW_A_PROPERTY_NAME_WHEN_THE_CONTAINING_OBJ:
        u32 = 1312;
    pub const THE_BODY_OF_AN_IF_STATEMENT_CANNOT_BE_THE_EMPTY_STATEMENT: u32 = 1313;
    pub const GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_MODULE_FILES: u32 = 1314;
    pub const GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_DECLARATION_FILES: u32 = 1315;
    pub const GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_AT_TOP_LEVEL: u32 = 1316;
    pub const A_PARAMETER_PROPERTY_CANNOT_BE_DECLARED_USING_A_REST_PARAMETER: u32 = 1317;
    pub const AN_ABSTRACT_ACCESSOR_CANNOT_HAVE_AN_IMPLEMENTATION: u32 = 1318;
    pub const A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE: u32 = 1319;
    pub const TYPE_OF_AWAIT_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLA:
        u32 = 1320;
    pub const TYPE_OF_YIELD_OPERAND_IN_AN_ASYNC_GENERATOR_MUST_EITHER_BE_A_VALID_PROMISE_OR_MU:
        u32 = 1321;
    pub const TYPE_OF_ITERATED_ELEMENTS_OF_A_YIELD_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_M:
        u32 = 1322;
    pub const DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022: u32 =
        1323;
    pub const DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO: u32 =
        1324;
    pub const ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT: u32 = 1325;
    pub const THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR:
        u32 = 1326;
    pub const STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED: u32 = 1327;
    pub const PROPERTY_VALUE_CAN_ONLY_BE_STRING_LITERAL_NUMERIC_LITERAL_TRUE_FALSE_NULL_OBJECT:
        u32 = 1328;
    pub const ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT:
        u32 = 1329;
    pub const A_PROPERTY_OF_AN_INTERFACE_OR_TYPE_LITERAL_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MU:
        u32 = 1330;
    pub const A_PROPERTY_OF_A_CLASS_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_BOTH_STATIC_AND:
        u32 = 1331;
    pub const A_VARIABLE_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_CONST: u32 = 1332;
    pub const UNIQUE_SYMBOL_TYPES_MAY_NOT_BE_USED_ON_A_VARIABLE_DECLARATION_WITH_A_BINDING_NAM:
        u32 = 1333;
    pub const UNIQUE_SYMBOL_TYPES_ARE_ONLY_ALLOWED_ON_VARIABLES_IN_A_VARIABLE_STATEMENT: u32 = 1334;
    pub const UNIQUE_SYMBOL_TYPES_ARE_NOT_ALLOWED_HERE: u32 = 1335;
    pub const AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI:
        u32 = 1337;
    pub const INFER_DECLARATIONS_ARE_ONLY_PERMITTED_IN_THE_EXTENDS_CLAUSE_OF_A_CONDITIONAL_TYP:
        u32 = 1338;
    pub const MODULE_DOES_NOT_REFER_TO_A_VALUE_BUT_IS_USED_AS_A_VALUE_HERE: u32 = 1339;
    pub const MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I:
        u32 = 1340;
    pub const CLASS_CONSTRUCTOR_MAY_NOT_BE_AN_ACCESSOR: u32 = 1341;
    pub const THE_IMPORT_META_META_PROPERTY_IS_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_ES2020_E:
        u32 = 1343;
    pub const A_LABEL_IS_NOT_ALLOWED_HERE: u32 = 1344;
    pub const AN_EXPRESSION_OF_TYPE_VOID_CANNOT_BE_TESTED_FOR_TRUTHINESS: u32 = 1345;
    pub const THIS_PARAMETER_IS_NOT_ALLOWED_WITH_USE_STRICT_DIRECTIVE: u32 = 1346;
    pub const USE_STRICT_DIRECTIVE_CANNOT_BE_USED_WITH_NON_SIMPLE_PARAMETER_LIST: u32 = 1347;
    pub const NON_SIMPLE_PARAMETER_DECLARED_HERE: u32 = 1348;
    pub const USE_STRICT_DIRECTIVE_USED_HERE: u32 = 1349;
    pub const PRINT_THE_FINAL_CONFIGURATION_INSTEAD_OF_BUILDING: u32 = 1350;
    pub const AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL: u32 = 1351;
    pub const A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION: u32 = 1352;
    pub const A_BIGINT_LITERAL_MUST_BE_AN_INTEGER: u32 = 1353;
    pub const READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES: u32 = 1354;
    pub const A_CONST_ASSERTION_CAN_ONLY_BE_APPLIED_TO_REFERENCES_TO_ENUM_MEMBERS_OR_STRING_NU:
        u32 = 1355;
    pub const DID_YOU_MEAN_TO_MARK_THIS_FUNCTION_AS_ASYNC: u32 = 1356;
    pub const AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR: u32 = 1357;
    pub const TAGGED_TEMPLATE_EXPRESSIONS_ARE_NOT_PERMITTED_IN_AN_OPTIONAL_CHAIN: u32 = 1358;
    pub const IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE: u32 = 1359;
    pub const TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE: u32 = 1360;
    pub const CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE: u32 = 1361;
    pub const CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE: u32 = 1362;
    pub const A_TYPE_ONLY_IMPORT_CAN_SPECIFY_A_DEFAULT_IMPORT_OR_NAMED_BINDINGS_BUT_NOT_BOTH: u32 =
        1363;
    pub const CONVERT_TO_TYPE_ONLY_EXPORT: u32 = 1364;
    pub const CONVERT_ALL_RE_EXPORTED_TYPES_TO_TYPE_ONLY_EXPORTS: u32 = 1365;
    pub const SPLIT_INTO_TWO_SEPARATE_IMPORT_DECLARATIONS: u32 = 1366;
    pub const SPLIT_ALL_INVALID_TYPE_ONLY_IMPORTS: u32 = 1367;
    pub const CLASS_CONSTRUCTOR_MAY_NOT_BE_A_GENERATOR: u32 = 1368;
    pub const DID_YOU_MEAN: u32 = 1369;
    pub const AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS: u32 =
        1375;
    pub const WAS_IMPORTED_HERE: u32 = 1376;
    pub const WAS_EXPORTED_HERE: u32 = 1377;
    pub const TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES:
        u32 = 1378;
    pub const AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_EXPORTED_USING_EXPORT_TY:
        u32 = 1379;
    pub const AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_IMPORTED_USING_IMPORT_TY:
        u32 = 1380;
    pub const UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE: u32 = 1381;
    pub const UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT: u32 = 1382;
    pub const FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE: u32 = 1385;
    pub const CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE: u32 = 1386;
    pub const FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYPE: u32 =
        1387;
    pub const CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYP:
        u32 = 1388;
    pub const IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME: u32 = 1389;
    pub const IS_NOT_ALLOWED_AS_A_PARAMETER_NAME: u32 = 1390;
    pub const AN_IMPORT_ALIAS_CANNOT_USE_IMPORT_TYPE: u32 = 1392;
    pub const IMPORTED_VIA_FROM_FILE: u32 = 1393;
    pub const IMPORTED_VIA_FROM_FILE_WITH_PACKAGEID: u32 = 1394;
    pub const IMPORTED_VIA_FROM_FILE_TO_IMPORT_IMPORTHELPERS_AS_SPECIFIED_IN_COMPILEROPTIONS: u32 =
        1395;
    pub const IMPORTED_VIA_FROM_FILE_WITH_PACKAGEID_TO_IMPORT_IMPORTHELPERS_AS_SPECIFIED_IN_CO:
        u32 = 1396;
    pub const IMPORTED_VIA_FROM_FILE_TO_IMPORT_JSX_AND_JSXS_FACTORY_FUNCTIONS: u32 = 1397;
    pub const IMPORTED_VIA_FROM_FILE_WITH_PACKAGEID_TO_IMPORT_JSX_AND_JSXS_FACTORY_FUNCTIONS: u32 =
        1398;
    pub const FILE_IS_INCLUDED_VIA_IMPORT_HERE: u32 = 1399;
    pub const REFERENCED_VIA_FROM_FILE: u32 = 1400;
    pub const FILE_IS_INCLUDED_VIA_REFERENCE_HERE: u32 = 1401;
    pub const TYPE_LIBRARY_REFERENCED_VIA_FROM_FILE: u32 = 1402;
    pub const TYPE_LIBRARY_REFERENCED_VIA_FROM_FILE_WITH_PACKAGEID: u32 = 1403;
    pub const FILE_IS_INCLUDED_VIA_TYPE_LIBRARY_REFERENCE_HERE: u32 = 1404;
    pub const LIBRARY_REFERENCED_VIA_FROM_FILE: u32 = 1405;
    pub const FILE_IS_INCLUDED_VIA_LIBRARY_REFERENCE_HERE: u32 = 1406;
    pub const MATCHED_BY_INCLUDE_PATTERN_IN: u32 = 1407;
    pub const FILE_IS_MATCHED_BY_INCLUDE_PATTERN_SPECIFIED_HERE: u32 = 1408;
    pub const PART_OF_FILES_LIST_IN_TSCONFIG_JSON: u32 = 1409;
    pub const FILE_IS_MATCHED_BY_FILES_LIST_SPECIFIED_HERE: u32 = 1410;
    pub const OUTPUT_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_SPECIFIED: u32 = 1411;
    pub const OUTPUT_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_MODULE_IS_SPECIFIED_AS_NONE: u32 =
        1412;
    pub const FILE_IS_OUTPUT_FROM_REFERENCED_PROJECT_SPECIFIED_HERE: u32 = 1413;
    pub const SOURCE_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_SPECIFIED: u32 = 1414;
    pub const SOURCE_FROM_REFERENCED_PROJECT_INCLUDED_BECAUSE_MODULE_IS_SPECIFIED_AS_NONE: u32 =
        1415;
    pub const FILE_IS_SOURCE_FROM_REFERENCED_PROJECT_SPECIFIED_HERE: u32 = 1416;
    pub const ENTRY_POINT_OF_TYPE_LIBRARY_SPECIFIED_IN_COMPILEROPTIONS: u32 = 1417;
    pub const ENTRY_POINT_OF_TYPE_LIBRARY_SPECIFIED_IN_COMPILEROPTIONS_WITH_PACKAGEID: u32 = 1418;
    pub const FILE_IS_ENTRY_POINT_OF_TYPE_LIBRARY_SPECIFIED_HERE: u32 = 1419;
    pub const ENTRY_POINT_FOR_IMPLICIT_TYPE_LIBRARY: u32 = 1420;
    pub const ENTRY_POINT_FOR_IMPLICIT_TYPE_LIBRARY_WITH_PACKAGEID: u32 = 1421;
    pub const LIBRARY_SPECIFIED_IN_COMPILEROPTIONS: u32 = 1422;
    pub const FILE_IS_LIBRARY_SPECIFIED_HERE: u32 = 1423;
    pub const DEFAULT_LIBRARY: u32 = 1424;
    pub const DEFAULT_LIBRARY_FOR_TARGET: u32 = 1425;
    pub const FILE_IS_DEFAULT_LIBRARY_FOR_TARGET_SPECIFIED_HERE: u32 = 1426;
    pub const ROOT_FILE_SPECIFIED_FOR_COMPILATION: u32 = 1427;
    pub const FILE_IS_OUTPUT_OF_PROJECT_REFERENCE_SOURCE: u32 = 1428;
    pub const FILE_REDIRECTS_TO_FILE: u32 = 1429;
    pub const THE_FILE_IS_IN_THE_PROGRAM_BECAUSE: u32 = 1430;
    pub const FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A: u32 =
        1431;
    pub const TOP_LEVEL_FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES20:
        u32 = 1432;
    pub const NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS: u32 = 1433;
    pub const UNEXPECTED_KEYWORD_OR_IDENTIFIER: u32 = 1434;
    pub const UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN: u32 = 1435;
    pub const DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS: u32 =
        1436;
    pub const NAMESPACE_MUST_BE_GIVEN_A_NAME: u32 = 1437;
    pub const INTERFACE_MUST_BE_GIVEN_A_NAME: u32 = 1438;
    pub const TYPE_ALIAS_MUST_BE_GIVEN_A_NAME: u32 = 1439;
    pub const VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION: u32 = 1440;
    pub const CANNOT_START_A_FUNCTION_CALL_IN_A_TYPE_ANNOTATION: u32 = 1441;
    pub const EXPECTED_FOR_PROPERTY_INITIALIZER: u32 = 1442;
    pub const MODULE_DECLARATION_NAMES_MAY_ONLY_USE_OR_QUOTED_STRINGS: u32 = 1443;
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE:
        u32 = 1448;
    pub const PRESERVE_UNUSED_IMPORTED_VALUES_IN_THE_JAVASCRIPT_OUTPUT_THAT_WOULD_OTHERWISE_BE:
        u32 = 1449;
    pub const DYNAMIC_IMPORTS_CAN_ONLY_ACCEPT_A_MODULE_SPECIFIER_AND_AN_OPTIONAL_SET_OF_ATTRIB:
        u32 = 1450;
    pub const PRIVATE_IDENTIFIERS_ARE_ONLY_ALLOWED_IN_CLASS_BODIES_AND_MAY_ONLY_BE_USED_AS_PAR:
        u32 = 1451;
    pub const RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT: u32 = 1453;
    pub const RESOLUTION_MODE_CAN_ONLY_BE_SET_FOR_TYPE_ONLY_IMPORTS: u32 = 1454;
    pub const RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ASSERTIONS: u32 = 1455;
    pub const TYPE_IMPORT_ASSERTIONS_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM:
        u32 = 1456;
    pub const MATCHED_BY_DEFAULT_INCLUDE_PATTERN: u32 = 1457;
    pub const FILE_IS_ECMASCRIPT_MODULE_BECAUSE_HAS_FIELD_TYPE_WITH_VALUE_MODULE: u32 = 1458;
    pub const FILE_IS_COMMONJS_MODULE_BECAUSE_HAS_FIELD_TYPE_WHOSE_VALUE_IS_NOT_MODULE: u32 = 1459;
    pub const FILE_IS_COMMONJS_MODULE_BECAUSE_DOES_NOT_HAVE_FIELD_TYPE: u32 = 1460;
    pub const FILE_IS_COMMONJS_MODULE_BECAUSE_PACKAGE_JSON_WAS_NOT_FOUND: u32 = 1461;
    pub const RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ATTRIBUTES: u32 = 1463;
    pub const TYPE_IMPORT_ATTRIBUTES_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM:
        u32 = 1464;
    pub const THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM:
        u32 = 1470;
    pub const MODULE_CANNOT_BE_IMPORTED_USING_THIS_CONSTRUCT_THE_SPECIFIER_ONLY_RESOLVES_TO_AN:
        u32 = 1471;
    pub const CATCH_OR_FINALLY_EXPECTED: u32 = 1472;
    pub const AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE: u32 = 1473;
    pub const AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE: u32 = 1474;
    pub const CONTROL_WHAT_METHOD_IS_USED_TO_DETECT_MODULE_FORMAT_JS_FILES: u32 = 1475;
    pub const AUTO_TREAT_FILES_WITH_IMPORTS_EXPORTS_IMPORT_META_JSX_WITH_JSX_REACT_JSX_OR_ESM: u32 =
        1476;
    pub const AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS: u32 = 1477;
    pub const IDENTIFIER_OR_STRING_LITERAL_EXPECTED: u32 = 1478;
    pub const THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H:
        u32 = 1479;
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_CHANGE_ITS_FILE_EXTENSION_TO_OR_CRE:
        u32 = 1480;
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_CHANGE_ITS_FILE_EXTENSION_TO_OR_ADD:
        u32 = 1481;
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_ADD_THE_FIELD_TYPE_MODULE_TO: u32 = 1482;
    pub const TO_CONVERT_THIS_FILE_TO_AN_ECMASCRIPT_MODULE_CREATE_A_LOCAL_PACKAGE_JSON_FILE_WI:
        u32 = 1483;
    pub const IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA:
        u32 = 1484;
    pub const RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPOR:
        u32 = 1485;
    pub const DECORATOR_USED_BEFORE_EXPORT_HERE: u32 = 1486;
    pub const OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX: u32 = 1487;
    pub const ESCAPE_SEQUENCE_IS_NOT_ALLOWED: u32 = 1488;
    pub const DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED: u32 = 1489;
    pub const FILE_APPEARS_TO_BE_BINARY: u32 = 1490;
    pub const MODIFIER_CANNOT_APPEAR_ON_A_USING_DECLARATION: u32 = 1491;
    pub const DECLARATIONS_MAY_NOT_HAVE_BINDING_PATTERNS: u32 = 1492;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_USING_DECLARATION: u32 = 1493;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_AN_AWAIT_USING_DECLARATION: u32 =
        1494;
    pub const MODIFIER_CANNOT_APPEAR_ON_AN_AWAIT_USING_DECLARATION: u32 = 1495;
    pub const IDENTIFIER_STRING_LITERAL_OR_NUMBER_LITERAL_EXPECTED: u32 = 1496;
    pub const EXPRESSION_MUST_BE_ENCLOSED_IN_PARENTHESES_TO_BE_USED_AS_A_DECORATOR: u32 = 1497;
    pub const INVALID_SYNTAX_IN_DECORATOR: u32 = 1498;
    pub const UNKNOWN_REGULAR_EXPRESSION_FLAG: u32 = 1499;
    pub const DUPLICATE_REGULAR_EXPRESSION_FLAG: u32 = 1500;
    pub const THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER: u32 = 1501;
    pub const THE_UNICODE_U_FLAG_AND_THE_UNICODE_SETS_V_FLAG_CANNOT_BE_SET_SIMULTANEOUSLY: u32 =
        1502;
    pub const NAMED_CAPTURING_GROUPS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ES2018_OR_LATER: u32 = 1503;
    pub const SUBPATTERN_FLAGS_MUST_BE_PRESENT_WHEN_THERE_IS_A_MINUS_SIGN: u32 = 1504;
    pub const INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED: u32 = 1505;
    pub const NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER: u32 = 1506;
    pub const THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION: u32 = 1507;
    pub const UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH: u32 = 1508;
    pub const THIS_REGULAR_EXPRESSION_FLAG_CANNOT_BE_TOGGLED_WITHIN_A_SUBPATTERN: u32 = 1509;
    pub const K_MUST_BE_FOLLOWED_BY_A_CAPTURING_GROUP_NAME_ENCLOSED_IN_ANGLE_BRACKETS: u32 = 1510;
    pub const Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS: u32 = 1511;
    pub const C_MUST_BE_FOLLOWED_BY_AN_ASCII_LETTER: u32 = 1512;
    pub const UNDETERMINED_CHARACTER_ESCAPE: u32 = 1513;
    pub const EXPECTED_A_CAPTURING_GROUP_NAME: u32 = 1514;
    pub const NAMED_CAPTURING_GROUPS_WITH_THE_SAME_NAME_MUST_BE_MUTUALLY_EXCLUSIVE_TO_EACH_OTH:
        u32 = 1515;
    pub const A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS: u32 = 1516;
    pub const RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS: u32 = 1517;
    pub const ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID:
        u32 = 1518;
    pub const OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_I:
        u32 = 1519;
    pub const EXPECTED_A_CLASS_SET_OPERAND: u32 = 1520;
    pub const Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES: u32 = 1521;
    pub const A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO: u32 =
        1522;
    pub const EXPECTED_A_UNICODE_PROPERTY_NAME: u32 = 1523;
    pub const UNKNOWN_UNICODE_PROPERTY_NAME: u32 = 1524;
    pub const EXPECTED_A_UNICODE_PROPERTY_VALUE: u32 = 1525;
    pub const UNKNOWN_UNICODE_PROPERTY_VALUE: u32 = 1526;
    pub const EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE: u32 = 1527;
    pub const ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O:
        u32 = 1528;
    pub const UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE: u32 = 1529;
    pub const UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR:
        u32 = 1530;
    pub const MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES: u32 =
        1531;
    pub const THERE_IS_NO_CAPTURING_GROUP_NAMED_IN_THIS_REGULAR_EXPRESSION: u32 = 1532;
    pub const THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_ONLY_CAPTURIN:
        u32 = 1533;
    pub const THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_NO_CAPTURING: u32 =
        1534;
    pub const THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION: u32 = 1535;
    pub const OCTAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS_I:
        u32 = 1536;
    pub const DECIMAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS:
        u32 = 1537;
    pub const UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO:
        u32 = 1538;
    pub const A_BIGINT_LITERAL_CANNOT_BE_USED_AS_A_PROPERTY_NAME: u32 = 1539;
    pub const A_NAMESPACE_DECLARATION_SHOULD_NOT_BE_DECLARED_USING_THE_MODULE_KEYWORD_PLEASE_U:
        u32 = 1540;
    pub const TYPE_ONLY_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESO:
        u32 = 1541;
    pub const TYPE_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESOLUTIO:
        u32 = 1542;
    pub const IMPORTING_A_JSON_FILE_INTO_AN_ECMASCRIPT_MODULE_REQUIRES_A_TYPE_JSON_IMPORT_ATTR:
        u32 = 1543;
    pub const NAMED_IMPORTS_FROM_A_JSON_FILE_INTO_AN_ECMASCRIPT_MODULE_ARE_NOT_ALLOWED_WHEN_MO:
        u32 = 1544;
    pub const USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: u32 = 1545;
    pub const AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS: u32 = 1546;
    pub const USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_W:
        u32 = 1547;
    pub const AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTA:
        u32 = 1548;
    pub const IGNORE_THE_TSCONFIG_FOUND_AND_BUILD_WITH_COMMANDLINE_OPTIONS_AND_FILES: u32 = 1549;
    pub const THE_TYPES_OF_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES: u32 = 2200;
    pub const THE_TYPES_RETURNED_BY_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES: u32 = 2201;
    pub const CALL_SIGNATURE_RETURN_TYPES_AND_ARE_INCOMPATIBLE: u32 = 2202;
    pub const CONSTRUCT_SIGNATURE_RETURN_TYPES_AND_ARE_INCOMPATIBLE: u32 = 2203;
    pub const CALL_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_AND: u32 = 2204;
    pub const CONSTRUCT_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_AND: u32 = 2205;
    pub const THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_IMPORT_WHEN_IMPORT_TYPE_IS_USED_ON_I:
        u32 = 2206;
    pub const THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_EXPORT_WHEN_EXPORT_TYPE_IS_USED_ON_I:
        u32 = 2207;
    pub const THIS_TYPE_PARAMETER_MIGHT_NEED_AN_EXTENDS_CONSTRAINT: u32 = 2208;
    pub const THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_EXPORT_MAP_ENTRY_IN_FIL:
        u32 = 2209;
    pub const THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_IMPORT_MAP_ENTRY_IN_FIL:
        u32 = 2210;
    pub const ADD_EXTENDS_CONSTRAINT: u32 = 2211;
    pub const ADD_EXTENDS_CONSTRAINT_TO_ALL_TYPE_PARAMETERS: u32 = 2212;
    pub const INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN: u32 =
        2301;
    pub const STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS: u32 = 2302;
    pub const CIRCULAR_DEFINITION_OF_IMPORT_ALIAS: u32 = 2303;
    pub const CANNOT_FIND_NAME: u32 = 2304;
    pub const MODULE_HAS_NO_EXPORTED_MEMBER: u32 = 2305;
    pub const FILE_IS_NOT_A_MODULE: u32 = 2306;
    pub const CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS: u32 = 2307;
    pub const MODULE_HAS_ALREADY_EXPORTED_A_MEMBER_NAMED_CONSIDER_EXPLICITLY_RE_EXPORTING_TO_R:
        u32 = 2308;
    pub const AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_MODULE_WITH_OTHER_EXPORTED_ELEMENTS: u32 =
        2309;
    pub const TYPE_RECURSIVELY_REFERENCES_ITSELF_AS_A_BASE_TYPE: u32 = 2310;
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION: u32 = 2311;
    pub const AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH:
        u32 = 2312;
    pub const TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT: u32 = 2313;
    pub const GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S: u32 = 2314;
    pub const TYPE_IS_NOT_GENERIC: u32 = 2315;
    pub const GLOBAL_TYPE_MUST_BE_A_CLASS_OR_INTERFACE_TYPE: u32 = 2316;
    pub const GLOBAL_TYPE_MUST_HAVE_TYPE_PARAMETER_S: u32 = 2317;
    pub const CANNOT_FIND_GLOBAL_TYPE: u32 = 2318;
    pub const NAMED_PROPERTY_OF_TYPES_AND_ARE_NOT_IDENTICAL: u32 = 2319;
    pub const INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND: u32 = 2320;
    pub const EXCESSIVE_STACK_DEPTH_COMPARING_TYPES_AND: u32 = 2321;
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE: u32 = 2322;
    pub const CANNOT_REDECLARE_EXPORTED_VARIABLE: u32 = 2323;
    pub const PROPERTY_IS_MISSING_IN_TYPE: u32 = 2324;
    pub const PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE: u32 = 2325;
    pub const TYPES_OF_PROPERTY_ARE_INCOMPATIBLE: u32 = 2326;
    pub const PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE: u32 = 2327;
    pub const TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE: u32 = 2328;
    pub const INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE: u32 = 2329;
    pub const AND_INDEX_SIGNATURES_ARE_INCOMPATIBLE: u32 = 2330;
    pub const THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY: u32 = 2331;
    pub const THIS_CANNOT_BE_REFERENCED_IN_CURRENT_LOCATION: u32 = 2332;
    pub const THIS_CANNOT_BE_REFERENCED_IN_A_STATIC_PROPERTY_INITIALIZER: u32 = 2334;
    pub const SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS: u32 = 2335;
    pub const SUPER_CANNOT_BE_REFERENCED_IN_CONSTRUCTOR_ARGUMENTS: u32 = 2336;
    pub const SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE:
        u32 = 2337;
    pub const SUPER_PROPERTY_ACCESS_IS_PERMITTED_ONLY_IN_A_CONSTRUCTOR_MEMBER_FUNCTION_OR_MEMB:
        u32 = 2338;
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE: u32 = 2339;
    pub const ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER:
        u32 = 2340;
    pub const PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS: u32 = 2341;
    pub const THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WHICH_DOES_NOT_EXIST_IN_CONSIDER_U:
        u32 = 2343;
    pub const TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT: u32 = 2344;
    pub const ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE: u32 = 2345;
    pub const CALL_TARGET_DOES_NOT_CONTAIN_ANY_SIGNATURES: u32 = 2346;
    pub const UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS: u32 = 2347;
    pub const VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW: u32 = 2348;
    pub const THIS_EXPRESSION_IS_NOT_CALLABLE: u32 = 2349;
    pub const ONLY_A_VOID_FUNCTION_CAN_BE_CALLED_WITH_THE_NEW_KEYWORD: u32 = 2350;
    pub const THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE: u32 = 2351;
    pub const CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV:
        u32 = 2352;
    pub const OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE: u32 =
        2353;
    pub const THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_BUT_MODULE_CANNOT_BE_FOUND: u32 = 2354;
    pub const A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V:
        u32 = 2355;
    pub const AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE: u32 = 2356;
    pub const THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPER:
        u32 = 2357;
    pub const THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP:
        u32 = 2358;
    pub const THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLA:
        u32 = 2359;
    pub const THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT: u32 =
        2362;
    pub const THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT:
        u32 = 2363;
    pub const THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY: u32 =
        2364;
    pub const OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND: u32 = 2365;
    pub const FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE:
        u32 = 2366;
    pub const THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA:
        u32 = 2367;
    pub const TYPE_PARAMETER_NAME_CANNOT_BE: u32 = 2368;
    pub const A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION: u32 = 2369;
    pub const A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE: u32 = 2370;
    pub const A_PARAMETER_INITIALIZER_IS_ONLY_ALLOWED_IN_A_FUNCTION_OR_CONSTRUCTOR_IMPLEMENTAT:
        u32 = 2371;
    pub const PARAMETER_CANNOT_REFERENCE_ITSELF: u32 = 2372;
    pub const PARAMETER_CANNOT_REFERENCE_IDENTIFIER_DECLARED_AFTER_IT: u32 = 2373;
    pub const DUPLICATE_INDEX_SIGNATURE_FOR_TYPE: u32 = 2374;
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD:
        u32 = 2375;
    pub const A_SUPER_CALL_MUST_BE_THE_FIRST_STATEMENT_IN_THE_CONSTRUCTOR_TO_REFER_TO_SUPER_OR:
        u32 = 2376;
    pub const CONSTRUCTORS_FOR_DERIVED_CLASSES_MUST_CONTAIN_A_SUPER_CALL: u32 = 2377;
    pub const A_GET_ACCESSOR_MUST_RETURN_A_VALUE: u32 = 2378;
    pub const ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_WITH_EXACTOPTIONALPROPER:
        u32 = 2379;
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_EXPORTED_OR_NON_EXPORTED: u32 = 2383;
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT: u32 = 2384;
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED: u32 = 2385;
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED: u32 = 2386;
    pub const FUNCTION_OVERLOAD_MUST_BE_STATIC: u32 = 2387;
    pub const FUNCTION_OVERLOAD_MUST_NOT_BE_STATIC: u32 = 2388;
    pub const FUNCTION_IMPLEMENTATION_NAME_MUST_BE: u32 = 2389;
    pub const CONSTRUCTOR_IMPLEMENTATION_IS_MISSING: u32 = 2390;
    pub const FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION: u32 =
        2391;
    pub const MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED: u32 = 2392;
    pub const DUPLICATE_FUNCTION_IMPLEMENTATION: u32 = 2393;
    pub const THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE: u32 =
        2394;
    pub const INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL: u32 =
        2395;
    pub const DUPLICATE_IDENTIFIER_ARGUMENTS_COMPILER_USES_ARGUMENTS_TO_INITIALIZE_REST_PARAME:
        u32 = 2396;
    pub const DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER: u32 = 2397;
    pub const CONSTRUCTOR_CANNOT_BE_USED_AS_A_PARAMETER_PROPERTY_NAME: u32 = 2398;
    pub const DUPLICATE_IDENTIFIER_THIS_COMPILER_USES_VARIABLE_DECLARATION_THIS_TO_CAPTURE_THI:
        u32 = 2399;
    pub const EXPRESSION_RESOLVES_TO_VARIABLE_DECLARATION_THIS_THAT_COMPILER_USES_TO_CAPTURE_T:
        u32 = 2400;
    pub const A_SUPER_CALL_MUST_BE_A_ROOT_LEVEL_STATEMENT_WITHIN_A_CONSTRUCTOR_OF_A_DERIVED_CL:
        u32 = 2401;
    pub const EXPRESSION_RESOLVES_TO_SUPER_THAT_COMPILER_USES_TO_CAPTURE_BASE_CLASS_REFERENCE: u32 =
        2402;
    pub const SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP:
        u32 = 2403;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION: u32 = 2404;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY: u32 = 2405;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS:
        u32 = 2406;
    pub const THE_RIGHT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR: u32 =
        2407;
    pub const SETTERS_CANNOT_RETURN_A_VALUE: u32 = 2408;
    pub const RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF: u32 =
        2409;
    pub const THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A:
        u32 = 2410;
    pub const PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE: u32 = 2411;
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2:
        u32 = 2412;
    pub const INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE: u32 = 2413;
    pub const CLASS_NAME_CANNOT_BE: u32 = 2414;
    pub const CLASS_INCORRECTLY_EXTENDS_BASE_CLASS: u32 = 2415;
    pub const PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE: u32 = 2416;
    pub const CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE: u32 = 2417;
    pub const TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE: u32 = 2418;
    pub const TYPES_OF_CONSTRUCT_SIGNATURES_ARE_INCOMPATIBLE: u32 = 2419;
    pub const CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE: u32 = 2420;
    pub const A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S:
        u32 = 2422;
    pub const CLASS_DEFINES_INSTANCE_MEMBER_FUNCTION_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE:
        u32 = 2423;
    pub const CLASS_DEFINES_INSTANCE_MEMBER_PROPERTY_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE:
        u32 = 2425;
    pub const CLASS_DEFINES_INSTANCE_MEMBER_ACCESSOR_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE:
        u32 = 2426;
    pub const INTERFACE_NAME_CANNOT_BE: u32 = 2427;
    pub const ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS: u32 = 2428;
    pub const INTERFACE_INCORRECTLY_EXTENDS_INTERFACE: u32 = 2430;
    pub const ENUM_NAME_CANNOT_BE: u32 = 2431;
    pub const IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ:
        u32 = 2432;
    pub const A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W:
        u32 = 2433;
    pub const A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC:
        u32 = 2434;
    pub const AMBIENT_MODULES_CANNOT_BE_NESTED_IN_OTHER_MODULES_OR_NAMESPACES: u32 = 2435;
    pub const AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME: u32 = 2436;
    pub const MODULE_IS_HIDDEN_BY_A_LOCAL_DECLARATION_WITH_THE_SAME_NAME: u32 = 2437;
    pub const IMPORT_NAME_CANNOT_BE: u32 = 2438;
    pub const IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M:
        u32 = 2439;
    pub const IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF: u32 = 2440;
    pub const DUPLICATE_IDENTIFIER_COMPILER_RESERVES_NAME_IN_TOP_LEVEL_SCOPE_OF_A_MODULE: u32 =
        2441;
    pub const TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY: u32 = 2442;
    pub const PROPERTY_IS_PROTECTED_BUT_TYPE_IS_NOT_A_CLASS_DERIVED_FROM: u32 = 2443;
    pub const PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE: u32 = 2444;
    pub const PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES: u32 = 2445;
    pub const PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A:
        u32 = 2446;
    pub const THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD: u32 = 2447;
    pub const BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION: u32 = 2448;
    pub const CLASS_USED_BEFORE_ITS_DECLARATION: u32 = 2449;
    pub const ENUM_USED_BEFORE_ITS_DECLARATION: u32 = 2450;
    pub const CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE: u32 = 2451;
    pub const AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME: u32 = 2452;
    pub const VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED: u32 = 2454;
    pub const TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF: u32 = 2456;
    pub const TYPE_ALIAS_NAME_CANNOT_BE: u32 = 2457;
    pub const AN_AMD_MODULE_CANNOT_HAVE_MULTIPLE_NAME_ASSIGNMENTS: u32 = 2458;
    pub const MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED: u32 = 2459;
    pub const MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS: u32 = 2460;
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE: u32 = 2461;
    pub const A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN: u32 = 2462;
    pub const A_BINDING_PATTERN_PARAMETER_CANNOT_BE_OPTIONAL_IN_AN_IMPLEMENTATION_SIGNATURE: u32 =
        2463;
    pub const A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY: u32 = 2464;
    pub const THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME: u32 = 2465;
    pub const SUPER_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME: u32 = 2466;
    pub const A_COMPUTED_PROPERTY_NAME_CANNOT_REFERENCE_A_TYPE_PARAMETER_FROM_ITS_CONTAINING_T:
        u32 = 2467;
    pub const CANNOT_FIND_GLOBAL_VALUE: u32 = 2468;
    pub const THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL: u32 = 2469;
    pub const SPREAD_OPERATOR_IN_NEW_EXPRESSIONS_IS_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_5:
        u32 = 2472;
    pub const ENUM_DECLARATIONS_MUST_ALL_BE_CONST_OR_NON_CONST: u32 = 2473;
    pub const CONST_ENUM_MEMBER_INITIALIZERS_MUST_BE_CONSTANT_EXPRESSIONS: u32 = 2474;
    pub const CONST_ENUMS_CAN_ONLY_BE_USED_IN_PROPERTY_OR_INDEX_ACCESS_EXPRESSIONS_OR_THE_RIGH:
        u32 = 2475;
    pub const A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL: u32 = 2476;
    pub const CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_A_NON_FINITE_VALUE: u32 = 2477;
    pub const CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_DISALLOWED_VALUE_NAN: u32 = 2478;
    pub const LET_IS_NOT_ALLOWED_TO_BE_USED_AS_A_NAME_IN_LET_OR_CONST_DECLARATIONS: u32 = 2480;
    pub const CANNOT_INITIALIZE_OUTER_SCOPED_VARIABLE_IN_THE_SAME_SCOPE_AS_BLOCK_SCOPED_DECLAR:
        u32 = 2481;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION: u32 = 2483;
    pub const EXPORT_DECLARATION_CONFLICTS_WITH_EXPORTED_DECLARATION_OF: u32 = 2484;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS:
        u32 = 2487;
    pub const TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR: u32 = 2488;
    pub const AN_ITERATOR_MUST_HAVE_A_NEXT_METHOD: u32 = 2489;
    pub const THE_TYPE_RETURNED_BY_THE_METHOD_OF_AN_ITERATOR_MUST_HAVE_A_VALUE_PROPERTY: u32 = 2490;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN: u32 =
        2491;
    pub const CANNOT_REDECLARE_IDENTIFIER_IN_CATCH_CLAUSE: u32 = 2492;
    pub const TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX: u32 = 2493;
    pub const USING_A_STRING_IN_A_FOR_OF_STATEMENT_IS_ONLY_SUPPORTED_IN_ECMASCRIPT_5_AND_HIGHE:
        u32 = 2494;
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE: u32 = 2495;
    pub const THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U:
        u32 = 2496;
    pub const THIS_MODULE_CAN_ONLY_BE_REFERENCED_WITH_ECMASCRIPT_IMPORTS_EXPORTS_BY_TURNING_ON:
        u32 = 2497;
    pub const MODULE_USES_EXPORT_AND_CANNOT_BE_USED_WITH_EXPORT: u32 = 2498;
    pub const AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG:
        u32 = 2499;
    pub const A_CLASS_CAN_ONLY_IMPLEMENT_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARGUM:
        u32 = 2500;
    pub const A_REST_ELEMENT_CANNOT_CONTAIN_A_BINDING_PATTERN: u32 = 2501;
    pub const IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_TYPE_ANNOTATION: u32 = 2502;
    pub const CANNOT_FIND_NAMESPACE: u32 = 2503;
    pub const TYPE_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR: u32 =
        2504;
    pub const A_GENERATOR_CANNOT_HAVE_A_VOID_TYPE_ANNOTATION: u32 = 2505;
    pub const IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION: u32 = 2506;
    pub const TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE: u32 = 2507;
    pub const NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS: u32 = 2508;
    pub const BASE_CONSTRUCTOR_RETURN_TYPE_IS_NOT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYP:
        u32 = 2509;
    pub const BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE: u32 = 2510;
    pub const CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS: u32 = 2511;
    pub const OVERLOAD_SIGNATURES_MUST_ALL_BE_ABSTRACT_OR_NON_ABSTRACT: u32 = 2512;
    pub const ABSTRACT_METHOD_IN_CLASS_CANNOT_BE_ACCESSED_VIA_SUPER_EXPRESSION: u32 = 2513;
    pub const A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE: u32 = 2514;
    pub const NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS: u32 =
        2515;
    pub const ALL_DECLARATIONS_OF_AN_ABSTRACT_METHOD_MUST_BE_CONSECUTIVE: u32 = 2516;
    pub const CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE: u32 =
        2517;
    pub const A_THIS_BASED_TYPE_GUARD_IS_NOT_COMPATIBLE_WITH_A_PARAMETER_BASED_TYPE_GUARD: u32 =
        2518;
    pub const AN_ASYNC_ITERATOR_MUST_HAVE_A_NEXT_METHOD: u32 = 2519;
    pub const DUPLICATE_IDENTIFIER_COMPILER_USES_DECLARATION_TO_SUPPORT_ASYNC_FUNCTIONS: u32 = 2520;
    pub const THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5: u32 =
        2522;
    pub const YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER: u32 = 2523;
    pub const AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER: u32 = 2524;
    pub const A_THIS_TYPE_IS_AVAILABLE_ONLY_IN_A_NON_STATIC_MEMBER_OF_A_CLASS_OR_INTERFACE: u32 =
        2526;
    pub const THE_INFERRED_TYPE_OF_REFERENCES_AN_INACCESSIBLE_TYPE_A_TYPE_ANNOTATION_IS_NECESS:
        u32 = 2527;
    pub const A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS: u32 = 2528;
    pub const DUPLICATE_IDENTIFIER_COMPILER_RESERVES_NAME_IN_TOP_LEVEL_SCOPE_OF_A_MODULE_CONTA:
        u32 = 2529;
    pub const PROPERTY_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE: u32 = 2530;
    pub const OBJECT_IS_POSSIBLY_NULL: u32 = 2531;
    pub const OBJECT_IS_POSSIBLY_UNDEFINED: u32 = 2532;
    pub const OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED: u32 = 2533;
    pub const A_FUNCTION_RETURNING_NEVER_CANNOT_HAVE_A_REACHABLE_END_POINT: u32 = 2534;
    pub const TYPE_CANNOT_BE_USED_TO_INDEX_TYPE: u32 = 2536;
    pub const TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE: u32 = 2537;
    pub const TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE: u32 = 2538;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_NOT_A_VARIABLE: u32 = 2539;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY: u32 = 2540;
    pub const INDEX_SIGNATURE_IN_TYPE_ONLY_PERMITS_READING: u32 = 2542;
    pub const DUPLICATE_IDENTIFIER_NEWTARGET_COMPILER_USES_VARIABLE_DECLARATION_NEWTARGET_TO_C:
        u32 = 2543;
    pub const EXPRESSION_RESOLVES_TO_VARIABLE_DECLARATION_NEWTARGET_THAT_COMPILER_USES_TO_CAPT:
        u32 = 2544;
    pub const A_MIXIN_CLASS_MUST_HAVE_A_CONSTRUCTOR_WITH_A_SINGLE_REST_PARAMETER_OF_TYPE_ANY: u32 =
        2545;
    pub const THE_TYPE_RETURNED_BY_THE_METHOD_OF_AN_ASYNC_ITERATOR_MUST_BE_A_PROMISE_FOR_A_TYP:
        u32 = 2547;
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE_OR_DOES_NOT_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS:
        u32 = 2548;
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE_OR_DOES_NOT_HAVE_A_SYMBOL_ITERATOR_ME:
        u32 = 2549;
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CH:
        u32 = 2550;
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN: u32 = 2551;
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN: u32 = 2552;
    pub const COMPUTED_VALUES_ARE_NOT_PERMITTED_IN_AN_ENUM_WITH_STRING_VALUED_MEMBERS: u32 = 2553;
    pub const EXPECTED_ARGUMENTS_BUT_GOT: u32 = 2554;
    pub const EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT: u32 = 2555;
    pub const A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER:
        u32 = 2556;
    pub const EXPECTED_TYPE_ARGUMENTS_BUT_GOT: u32 = 2558;
    pub const TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE: u32 = 2559;
    pub const VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT: u32 =
        2560;
    pub const OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID: u32 =
        2561;
    pub const BASE_CLASS_EXPRESSIONS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS: u32 = 2562;
    pub const THE_CONTAINING_FUNCTION_OR_MODULE_BODY_IS_TOO_LARGE_FOR_CONTROL_FLOW_ANALYSIS: u32 =
        2563;
    pub const PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR: u32 =
        2564;
    pub const PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED: u32 = 2565;
    pub const A_REST_ELEMENT_CANNOT_HAVE_A_PROPERTY_NAME: u32 = 2566;
    pub const ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS: u32 =
        2567;
    pub const PROPERTY_MAY_NOT_EXIST_ON_TYPE_DID_YOU_MEAN: u32 = 2568;
    pub const COULD_NOT_FIND_NAME_DID_YOU_MEAN: u32 = 2570;
    pub const OBJECT_IS_OF_TYPE_UNKNOWN: u32 = 2571;
    pub const A_REST_ELEMENT_TYPE_MUST_BE_AN_ARRAY_TYPE: u32 = 2574;
    pub const NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM:
        u32 = 2575;
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD:
        u32 = 2576;
    pub const RETURN_TYPE_ANNOTATION_CIRCULARLY_REFERENCES_ITSELF: u32 = 2577;
    pub const UNUSED_TS_EXPECT_ERROR_DIRECTIVE: u32 = 2578;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE:
        u32 = 2580;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA:
        u32 = 2581;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N:
        u32 = 2582;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB: u32 =
        2583;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2:
        u32 = 2584;
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO:
        u32 = 2585;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CONSTANT: u32 = 2588;
    pub const TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE: u32 = 2589;
    pub const EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT: u32 = 2590;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2:
        u32 = 2591;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA_2:
        u32 = 2592;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N_2:
        u32 = 2593;
    pub const THIS_MODULE_IS_DECLARED_WITH_EXPORT_AND_CAN_ONLY_BE_USED_WITH_A_DEFAULT_IMPORT_W:
        u32 = 2594;
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_A_DEFAULT_IMPORT: u32 = 2595;
    pub const CAN_ONLY_BE_IMPORTED_BY_TURNING_ON_THE_ESMODULEINTEROP_FLAG_AND_USING_A_DEFAULT: u32 =
        2596;
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_A_REQUIRE_CALL_OR_BY_USING_A_DEFAULT_IMPORT: u32 = 2597;
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_A_REQUIRE_CALL_OR_BY_TURNING_ON_THE_ESMODULEINTERO:
        u32 = 2598;
    pub const JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_THE_GLOBAL_TYPE_JSX_ELEMENT_DOES_NOT:
        u32 = 2602;
    pub const PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE: u32 = 2603;
    pub const JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES: u32 = 2604;
    pub const PROPERTY_OF_JSX_SPREAD_ATTRIBUTE_IS_NOT_ASSIGNABLE_TO_TARGET_PROPERTY: u32 = 2606;
    pub const JSX_ELEMENT_CLASS_DOES_NOT_SUPPORT_ATTRIBUTES_BECAUSE_IT_DOES_NOT_HAVE_A_PROPERT:
        u32 = 2607;
    pub const THE_GLOBAL_TYPE_JSX_MAY_NOT_HAVE_MORE_THAN_ONE_PROPERTY: u32 = 2608;
    pub const JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE: u32 = 2609;
    pub const IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP:
        u32 = 2610;
    pub const IS_DEFINED_AS_A_PROPERTY_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_ACCESSOR: u32 =
        2611;
    pub const PROPERTY_WILL_OVERWRITE_THE_BASE_PROPERTY_IN_IF_THIS_IS_INTENTIONAL_ADD_AN_INITI:
        u32 = 2612;
    pub const MODULE_HAS_NO_DEFAULT_EXPORT_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD: u32 = 2613;
    pub const MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD: u32 = 2614;
    pub const TYPE_OF_PROPERTY_CIRCULARLY_REFERENCES_ITSELF_IN_MAPPED_TYPE: u32 = 2615;
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_A_DEFAULT_IMPORT: u32 = 2616;
    pub const CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_BY_TURNING_ON_THE_ESMODULEINTERO:
        u32 = 2617;
    pub const SOURCE_HAS_ELEMENT_S_BUT_TARGET_REQUIRES: u32 = 2618;
    pub const SOURCE_HAS_ELEMENT_S_BUT_TARGET_ALLOWS_ONLY: u32 = 2619;
    pub const TARGET_REQUIRES_ELEMENT_S_BUT_SOURCE_MAY_HAVE_FEWER: u32 = 2620;
    pub const TARGET_ALLOWS_ONLY_ELEMENT_S_BUT_SOURCE_MAY_HAVE_MORE: u32 = 2621;
    pub const SOURCE_PROVIDES_NO_MATCH_FOR_REQUIRED_ELEMENT_AT_POSITION_IN_TARGET: u32 = 2623;
    pub const SOURCE_PROVIDES_NO_MATCH_FOR_VARIADIC_ELEMENT_AT_POSITION_IN_TARGET: u32 = 2624;
    pub const VARIADIC_ELEMENT_AT_POSITION_IN_SOURCE_DOES_NOT_MATCH_ELEMENT_AT_POSITION_IN_TAR:
        u32 = 2625;
    pub const TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET: u32 =
        2626;
    pub const TYPE_AT_POSITIONS_THROUGH_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_T:
        u32 = 2627;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_ENUM: u32 = 2628;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CLASS: u32 = 2629;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_FUNCTION: u32 = 2630;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_NAMESPACE: u32 = 2631;
    pub const CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_IMPORT: u32 = 2632;
    pub const JSX_PROPERTY_ACCESS_EXPRESSIONS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES: u32 = 2633;
    pub const INDEX_SIGNATURES_ARE_INCOMPATIBLE: u32 = 2634;
    pub const TYPE_HAS_NO_SIGNATURES_FOR_WHICH_THE_TYPE_ARGUMENT_LIST_IS_APPLICABLE: u32 = 2635;
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_IMPLIED_BY_VARIANCE_ANNOTATION: u32 = 2636;
    pub const VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS:
        u32 = 2637;
    pub const TYPE_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND:
        u32 = 2638;
    pub const REACT_COMPONENTS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES: u32 = 2639;
    pub const CANNOT_AUGMENT_MODULE_WITH_VALUE_EXPORTS_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENT:
        u32 = 2649;
    pub const NON_ABSTRACT_CLASS_EXPRESSION_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBE:
        u32 = 2650;
    pub const A_MEMBER_INITIALIZER_IN_A_ENUM_DECLARATION_CANNOT_REFERENCE_MEMBERS_DECLARED_AFT:
        u32 = 2651;
    pub const MERGED_DECLARATION_CANNOT_INCLUDE_A_DEFAULT_EXPORT_DECLARATION_CONSIDER_ADDING_A:
        u32 = 2652;
    pub const NON_ABSTRACT_CLASS_EXPRESSION_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM: u32 =
        2653;
    pub const NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF: u32 =
        2654;
    pub const NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF_AND_M:
        u32 = 2655;
    pub const NON_ABSTRACT_CLASS_EXPRESSION_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBE_2:
        u32 = 2656;
    pub const JSX_EXPRESSIONS_MUST_HAVE_ONE_PARENT_ELEMENT: u32 = 2657;
    pub const TYPE_PROVIDES_NO_MATCH_FOR_THE_SIGNATURE: u32 = 2658;
    pub const SUPER_IS_ONLY_ALLOWED_IN_MEMBERS_OF_OBJECT_LITERAL_EXPRESSIONS_WHEN_OPTION_TARGE:
        u32 = 2659;
    pub const SUPER_CAN_ONLY_BE_REFERENCED_IN_MEMBERS_OF_DERIVED_CLASSES_OR_OBJECT_LITERAL_EXP:
        u32 = 2660;
    pub const CANNOT_EXPORT_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE: u32 = 2661;
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER: u32 = 2662;
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS: u32 = 2663;
    pub const INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND: u32 = 2664;
    pub const INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_RESOLVES_TO_AN_UNTYPED_MODULE_AT_WHIC:
        u32 = 2665;
    pub const EXPORTS_AND_EXPORT_ASSIGNMENTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS: u32 = 2666;
    pub const IMPORTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS_CONSIDER_MOVING_THEM_TO_THE_EN:
        u32 = 2667;
    pub const EXPORT_MODIFIER_CANNOT_BE_APPLIED_TO_AMBIENT_MODULES_AND_MODULE_AUGMENTATIONS_SI:
        u32 = 2668;
    pub const AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL:
        u32 = 2669;
    pub const AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_SHOULD_HAVE_DECLARE_MODIFIER_UNLESS_THEY_APPE:
        u32 = 2670;
    pub const CANNOT_AUGMENT_MODULE_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENTITY: u32 = 2671;
    pub const CANNOT_ASSIGN_A_CONSTRUCTOR_TYPE_TO_A_CONSTRUCTOR_TYPE: u32 = 2672;
    pub const CONSTRUCTOR_OF_CLASS_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATION:
        u32 = 2673;
    pub const CONSTRUCTOR_OF_CLASS_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATI:
        u32 = 2674;
    pub const CANNOT_EXTEND_A_CLASS_CLASS_CONSTRUCTOR_IS_MARKED_AS_PRIVATE: u32 = 2675;
    pub const ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT: u32 = 2676;
    pub const A_TYPE_PREDICATES_TYPE_MUST_BE_ASSIGNABLE_TO_ITS_PARAMETERS_TYPE: u32 = 2677;
    pub const TYPE_IS_NOT_COMPARABLE_TO_TYPE: u32 = 2678;
    pub const A_FUNCTION_THAT_IS_CALLED_WITH_THE_NEW_KEYWORD_CANNOT_HAVE_A_THIS_TYPE_THAT_IS_V:
        u32 = 2679;
    pub const A_PARAMETER_MUST_BE_THE_FIRST_PARAMETER: u32 = 2680;
    pub const A_CONSTRUCTOR_CANNOT_HAVE_A_THIS_PARAMETER: u32 = 2681;
    pub const THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION: u32 = 2683;
    pub const THE_THIS_CONTEXT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_METHODS_THIS_OF_TYPE: u32 = 2684;
    pub const THE_THIS_TYPES_OF_EACH_SIGNATURE_ARE_INCOMPATIBLE: u32 = 2685;
    pub const REFERS_TO_A_UMD_GLOBAL_BUT_THE_CURRENT_FILE_IS_A_MODULE_CONSIDER_ADDING_AN_IMPOR:
        u32 = 2686;
    pub const ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS: u32 = 2687;
    pub const CANNOT_FIND_TYPE_DEFINITION_FILE_FOR: u32 = 2688;
    pub const CANNOT_EXTEND_AN_INTERFACE_DID_YOU_MEAN_IMPLEMENTS: u32 = 2689;
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DID_YOU_MEAN_TO_USE_IN: u32 =
        2690;
    pub const IS_A_PRIMITIVE_BUT_IS_A_WRAPPER_OBJECT_PREFER_USING_WHEN_POSSIBLE: u32 = 2692;
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE: u32 = 2693;
    pub const NAMESPACE_HAS_NO_EXPORTED_MEMBER: u32 = 2694;
    pub const LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS: u32 = 2695;
    pub const THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN:
        u32 = 2696;
    pub const AN_ASYNC_FUNCTION_OR_METHOD_MUST_RETURN_A_PROMISE_MAKE_SURE_YOU_HAVE_A_DECLARATI:
        u32 = 2697;
    pub const SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES: u32 = 2698;
    pub const STATIC_PROPERTY_CONFLICTS_WITH_BUILT_IN_PROPERTY_FUNCTION_OF_CONSTRUCTOR_FUNCTIO:
        u32 = 2699;
    pub const REST_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES: u32 = 2700;
    pub const THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS: u32 =
        2701;
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_NAMESPACE_HERE: u32 = 2702;
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE: u32 = 2703;
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_READ_ONLY_PROPERTY: u32 = 2704;
    pub const AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO:
        u32 = 2705;
    pub const REQUIRED_TYPE_PARAMETERS_MAY_NOT_FOLLOW_OPTIONAL_TYPE_PARAMETERS: u32 = 2706;
    pub const GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS: u32 = 2707;
    pub const CANNOT_USE_NAMESPACE_AS_A_VALUE: u32 = 2708;
    pub const CANNOT_USE_NAMESPACE_AS_A_TYPE: u32 = 2709;
    pub const ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN: u32 = 2710;
    pub const A_DYNAMIC_IMPORT_CALL_RETURNS_A_PROMISE_MAKE_SURE_YOU_HAVE_A_DECLARATION_FOR_PRO:
        u32 = 2711;
    pub const A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE:
        u32 = 2712;
    pub const CANNOT_ACCESS_BECAUSE_IS_A_TYPE_BUT_NOT_A_NAMESPACE_DID_YOU_MEAN_TO_RETRIEVE_THE:
        u32 = 2713;
    pub const THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I:
        u32 = 2714;
    pub const ABSTRACT_PROPERTY_IN_CLASS_CANNOT_BE_ACCESSED_IN_THE_CONSTRUCTOR: u32 = 2715;
    pub const TYPE_PARAMETER_HAS_A_CIRCULAR_DEFAULT: u32 = 2716;
    pub const SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP:
        u32 = 2717;
    pub const DUPLICATE_PROPERTY: u32 = 2718;
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY:
        u32 = 2719;
    pub const CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER:
        u32 = 2720;
    pub const CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL: u32 = 2721;
    pub const CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED: u32 = 2722;
    pub const CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL_OR_UNDEFINED: u32 = 2723;
    pub const HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN: u32 = 2724;
    pub const CLASS_NAME_CANNOT_BE_OBJECT_WHEN_TARGETING_ES5_AND_ABOVE_WITH_MODULE: u32 = 2725;
    pub const CANNOT_FIND_LIB_DEFINITION_FOR: u32 = 2726;
    pub const CANNOT_FIND_LIB_DEFINITION_FOR_DID_YOU_MEAN: u32 = 2727;
    pub const IS_DECLARED_HERE: u32 = 2728;
    pub const PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION: u32 = 2729;
    pub const AN_ARROW_FUNCTION_CANNOT_HAVE_A_THIS_PARAMETER: u32 = 2730;
    pub const IMPLICIT_CONVERSION_OF_A_SYMBOL_TO_A_STRING_WILL_FAIL_AT_RUNTIME_CONSIDER_WRAPPI:
        u32 = 2731;
    pub const CANNOT_FIND_MODULE_CONSIDER_USING_RESOLVEJSONMODULE_TO_IMPORT_MODULE_WITH_JSON_E:
        u32 = 2732;
    pub const PROPERTY_WAS_ALSO_DECLARED_HERE: u32 = 2733;
    pub const ARE_YOU_MISSING_A_SEMICOLON: u32 = 2734;
    pub const DID_YOU_MEAN_FOR_TO_BE_CONSTRAINED_TO_TYPE_NEW_ARGS_ANY: u32 = 2735;
    pub const OPERATOR_CANNOT_BE_APPLIED_TO_TYPE: u32 = 2736;
    pub const BIGINT_LITERALS_ARE_NOT_AVAILABLE_WHEN_TARGETING_LOWER_THAN_ES2020: u32 = 2737;
    pub const AN_OUTER_VALUE_OF_THIS_IS_SHADOWED_BY_THIS_CONTAINER: u32 = 2738;
    pub const TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE: u32 = 2739;
    pub const TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE: u32 = 2740;
    pub const PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE: u32 = 2741;
    pub const THE_INFERRED_TYPE_OF_CANNOT_BE_NAMED_WITHOUT_A_REFERENCE_TO_THIS_IS_LIKELY_NOT_P:
        u32 = 2742;
    pub const NO_OVERLOAD_EXPECTS_TYPE_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR: u32 =
        2743;
    pub const TYPE_PARAMETER_DEFAULTS_CAN_ONLY_REFERENCE_PREVIOUSLY_DECLARED_TYPE_PARAMETERS: u32 =
        2744;
    pub const THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING:
        u32 = 2745;
    pub const THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO:
        u32 = 2746;
    pub const COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU:
        u32 = 2747;
    pub const CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED: u32 = 2748;
    pub const REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF: u32 = 2749;
    pub const THE_IMPLEMENTATION_SIGNATURE_IS_DECLARED_HERE: u32 = 2750;
    pub const CIRCULARITY_ORIGINATES_IN_TYPE_AT_THIS_LOCATION: u32 = 2751;
    pub const THE_FIRST_EXPORT_DEFAULT_IS_HERE: u32 = 2752;
    pub const ANOTHER_EXPORT_DEFAULT_IS_HERE: u32 = 2753;
    pub const SUPER_MAY_NOT_USE_TYPE_ARGUMENTS: u32 = 2754;
    pub const NO_CONSTITUENT_OF_TYPE_IS_CALLABLE: u32 = 2755;
    pub const NOT_ALL_CONSTITUENTS_OF_TYPE_ARE_CALLABLE: u32 = 2756;
    pub const TYPE_HAS_NO_CALL_SIGNATURES: u32 = 2757;
    pub const EACH_MEMBER_OF_THE_UNION_TYPE_HAS_SIGNATURES_BUT_NONE_OF_THOSE_SIGNATURES_ARE_CO:
        u32 = 2758;
    pub const NO_CONSTITUENT_OF_TYPE_IS_CONSTRUCTABLE: u32 = 2759;
    pub const NOT_ALL_CONSTITUENTS_OF_TYPE_ARE_CONSTRUCTABLE: u32 = 2760;
    pub const TYPE_HAS_NO_CONSTRUCT_SIGNATURES: u32 = 2761;
    pub const EACH_MEMBER_OF_THE_UNION_TYPE_HAS_CONSTRUCT_SIGNATURES_BUT_NONE_OF_THOSE_SIGNATU:
        u32 = 2762;
    pub const CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_FO:
        u32 = 2763;
    pub const CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR:
        u32 = 2764;
    pub const CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR_2:
        u32 = 2765;
    pub const CANNOT_DELEGATE_ITERATION_TO_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPEC:
        u32 = 2766;
    pub const THE_PROPERTY_OF_AN_ITERATOR_MUST_BE_A_METHOD: u32 = 2767;
    pub const THE_PROPERTY_OF_AN_ASYNC_ITERATOR_MUST_BE_A_METHOD: u32 = 2768;
    pub const NO_OVERLOAD_MATCHES_THIS_CALL: u32 = 2769;
    pub const THE_LAST_OVERLOAD_GAVE_THE_FOLLOWING_ERROR: u32 = 2770;
    pub const THE_LAST_OVERLOAD_IS_DECLARED_HERE: u32 = 2771;
    pub const OVERLOAD_OF_GAVE_THE_FOLLOWING_ERROR: u32 = 2772;
    pub const DID_YOU_FORGET_TO_USE_AWAIT: u32 = 2773;
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_FUNCTION_IS_ALWAYS_DEFINED_DID:
        u32 = 2774;
    pub const ASSERTIONS_REQUIRE_EVERY_NAME_IN_THE_CALL_TARGET_TO_BE_DECLARED_WITH_AN_EXPLICIT:
        u32 = 2775;
    pub const ASSERTIONS_REQUIRE_THE_CALL_TARGET_TO_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME: u32 = 2776;
    pub const THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERT:
        u32 = 2777;
    pub const THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS: u32 =
        2778;
    pub const THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A:
        u32 = 2779;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS: u32 =
        2780;
    pub const THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS: u32 =
        2781;
    pub const NEEDS_AN_EXPLICIT_TYPE_ANNOTATION: u32 = 2782;
    pub const IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN: u32 = 2783;
    pub const GET_AND_SET_ACCESSORS_CANNOT_DECLARE_THIS_PARAMETERS: u32 = 2784;
    pub const THIS_SPREAD_ALWAYS_OVERWRITES_THIS_PROPERTY: u32 = 2785;
    pub const CANNOT_BE_USED_AS_A_JSX_COMPONENT: u32 = 2786;
    pub const ITS_RETURN_TYPE_IS_NOT_A_VALID_JSX_ELEMENT: u32 = 2787;
    pub const ITS_INSTANCE_TYPE_IS_NOT_A_VALID_JSX_ELEMENT: u32 = 2788;
    pub const ITS_ELEMENT_TYPE_IS_NOT_A_VALID_JSX_ELEMENT: u32 = 2789;
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL: u32 = 2790;
    pub const EXPONENTIATION_CANNOT_BE_PERFORMED_ON_BIGINT_VALUES_UNLESS_THE_TARGET_OPTION_IS: u32 =
        2791;
    pub const CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O:
        u32 = 2792;
    pub const THE_CALL_WOULD_HAVE_SUCCEEDED_AGAINST_THIS_IMPLEMENTATION_BUT_IMPLEMENTATION_SIG:
        u32 = 2793;
    pub const EXPECTED_ARGUMENTS_BUT_GOT_DID_YOU_FORGET_TO_INCLUDE_VOID_IN_YOUR_TYPE_ARGUMENT: u32 =
        2794;
    pub const THE_INTRINSIC_KEYWORD_CAN_ONLY_BE_USED_TO_DECLARE_COMPILER_PROVIDED_INTRINSIC_TY:
        u32 = 2795;
    pub const IT_IS_LIKELY_THAT_YOU_ARE_MISSING_A_COMMA_TO_SEPARATE_THESE_TWO_TEMPLATE_EXPRESS:
        u32 = 2796;
    pub const A_MIXIN_CLASS_THAT_EXTENDS_FROM_A_TYPE_VARIABLE_CONTAINING_AN_ABSTRACT_CONSTRUCT:
        u32 = 2797;
    pub const THE_DECLARATION_WAS_MARKED_AS_DEPRECATED_HERE: u32 = 2798;
    pub const TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT: u32 = 2799;
    pub const EXPRESSION_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT: u32 = 2800;
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_IS_ALWAYS_DEFINED: u32 = 2801;
    pub const TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH:
        u32 = 2802;
    pub const CANNOT_ASSIGN_TO_PRIVATE_METHOD_PRIVATE_METHODS_ARE_NOT_WRITABLE: u32 = 2803;
    pub const DUPLICATE_IDENTIFIER_STATIC_AND_INSTANCE_ELEMENTS_CANNOT_SHARE_THE_SAME_PRIVATE: u32 =
        2804;
    pub const PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER: u32 = 2806;
    pub const THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_WITH_PARAMETERS_WHICH_IS_NOT_COMPA:
        u32 = 2807;
    pub const A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER: u32 = 2808;
    pub const DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I:
        u32 = 2809;
    pub const EXPECTED_1_ARGUMENT_BUT_GOT_0_NEW_PROMISE_NEEDS_A_JSDOC_HINT_TO_PRODUCE_A_RESOLV:
        u32 = 2810;
    pub const INITIALIZER_FOR_PROPERTY: u32 = 2811;
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_INCLUDE: u32 =
        2812;
    pub const CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR: u32 = 2813;
    pub const FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT: u32 = 2814;
    pub const ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI:
        u32 = 2815;
    pub const CANNOT_USE_THIS_IN_A_STATIC_PROPERTY_INITIALIZER_OF_A_DECORATED_CLASS: u32 = 2816;
    pub const PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_A_CLASS_STATIC_BLO:
        u32 = 2817;
    pub const DUPLICATE_IDENTIFIER_COMPILER_RESERVES_NAME_WHEN_EMITTING_SUPER_REFERENCES_IN_ST:
        u32 = 2818;
    pub const NAMESPACE_NAME_CANNOT_BE: u32 = 2819;
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN: u32 = 2820;
    pub const IMPORT_ASSERTIONS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD:
        u32 = 2821;
    pub const IMPORT_ASSERTIONS_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS: u32 = 2822;
    pub const IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD:
        u32 = 2823;
    pub const CANNOT_FIND_NAMESPACE_DID_YOU_MEAN: u32 = 2833;
    pub const RELATIVE_IMPORT_PATHS_NEED_EXPLICIT_FILE_EXTENSIONS_IN_ECMASCRIPT_IMPORTS_WHEN_M:
        u32 = 2834;
    pub const RELATIVE_IMPORT_PATHS_NEED_EXPLICIT_FILE_EXTENSIONS_IN_ECMASCRIPT_IMPORTS_WHEN_M_2:
        u32 = 2835;
    pub const IMPORT_ASSERTIONS_ARE_NOT_ALLOWED_ON_STATEMENTS_THAT_COMPILE_TO_COMMONJS_REQUIRE:
        u32 = 2836;
    pub const IMPORT_ASSERTION_VALUES_MUST_BE_STRING_LITERAL_EXPRESSIONS: u32 = 2837;
    pub const ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_CONSTRAINTS: u32 = 2838;
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN_SINCE_JAVASCRIPT_COMPARES_OBJECTS_BY_REFERENCE:
        u32 = 2839;
    pub const AN_INTERFACE_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_EXTEND_OTHER_NAMED: u32 =
        2840;
    pub const IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION: u32 = 2842;
    pub const WE_CAN_ONLY_WRITE_A_TYPE_FOR_BY_ADDING_A_TYPE_FOR_THE_ENTIRE_PARAMETER_HERE: u32 =
        2843;
    pub const TYPE_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN_THE_CON:
        u32 = 2844;
    pub const THIS_CONDITION_WILL_ALWAYS_RETURN: u32 = 2845;
    pub const A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT:
        u32 = 2846;
    pub const THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP:
        u32 = 2848;
    pub const TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT: u32 = 2849;
    pub const THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI:
        u32 = 2850;
    pub const THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY:
        u32 = 2851;
    pub const AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE:
        u32 = 2852;
    pub const AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FIL:
        u32 = 2853;
    pub const TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET: u32 =
        2854;
    pub const CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA:
        u32 = 2855;
    pub const IMPORT_ATTRIBUTES_ARE_NOT_ALLOWED_ON_STATEMENTS_THAT_COMPILE_TO_COMMONJS_REQUIRE:
        u32 = 2856;
    pub const IMPORT_ATTRIBUTES_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS: u32 = 2857;
    pub const IMPORT_ATTRIBUTE_VALUES_MUST_BE_STRING_LITERAL_EXPRESSIONS: u32 = 2858;
    pub const EXCESSIVE_COMPLEXITY_COMPARING_TYPES_AND: u32 = 2859;
    pub const THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_ASSIGNABLE_TO_THE_FIRST_A:
        u32 = 2860;
    pub const AN_OBJECTS_SYMBOL_HASINSTANCE_METHOD_MUST_RETURN_A_BOOLEAN_VALUE_FOR_IT_TO_BE_US:
        u32 = 2861;
    pub const TYPE_IS_GENERIC_AND_CAN_ONLY_BE_INDEXED_FOR_READING: u32 = 2862;
    pub const A_CLASS_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_CLASSES_CAN_ONLY_EXTEND_CONSTRUCTABL:
        u32 = 2863;
    pub const A_CLASS_CANNOT_IMPLEMENT_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_IMPLEMENT_OTHER_NAMED:
        u32 = 2864;
    pub const IMPORT_CONFLICTS_WITH_LOCAL_VALUE_SO_MUST_BE_DECLARED_WITH_A_TYPE_ONLY_IMPORT_WH:
        u32 = 2865;
    pub const IMPORT_CONFLICTS_WITH_GLOBAL_VALUE_USED_IN_THIS_FILE_SO_MUST_BE_DECLARED_WITH_A: u32 =
        2866;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE: u32 =
        2867;
    pub const CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE_2:
        u32 = 2868;
    pub const RIGHT_OPERAND_OF_IS_UNREACHABLE_BECAUSE_THE_LEFT_OPERAND_IS_NEVER_NULLISH: u32 = 2869;
    pub const THIS_BINARY_EXPRESSION_IS_NEVER_NULLISH_ARE_YOU_MISSING_PARENTHESES: u32 = 2870;
    pub const THIS_EXPRESSION_IS_ALWAYS_NULLISH: u32 = 2871;
    pub const THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY: u32 = 2872;
    pub const THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY: u32 = 2873;
    pub const THIS_JSX_TAG_REQUIRES_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND: u32 = 2874;
    pub const THIS_JSX_TAG_REQUIRES_THE_MODULE_PATH_TO_EXIST_BUT_NONE_COULD_BE_FOUND_MAKE_SURE:
        u32 = 2875;
    pub const THIS_RELATIVE_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_LOOKS_LIKE_A_FILE_NAME:
        u32 = 2876;
    pub const THIS_IMPORT_USES_A_EXTENSION_TO_RESOLVE_TO_AN_INPUT_TYPESCRIPT_FILE_BUT_WILL_NOT:
        u32 = 2877;
    pub const THIS_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_RESOLVES_TO_ANOTHER_PROJECT_AND:
        u32 = 2878;
    pub const USING_JSX_FRAGMENTS_REQUIRES_FRAGMENT_FACTORY_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE:
        u32 = 2879;
    pub const IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS:
        u32 = 2880;
    pub const THIS_EXPRESSION_IS_NEVER_NULLISH: u32 = 2881;
    pub const CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF: u32 = 2882;
    pub const IMPORT_DECLARATION_IS_USING_PRIVATE_NAME: u32 = 4000;
    pub const TYPE_PARAMETER_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4002;
    pub const TYPE_PARAMETER_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4004;
    pub const TYPE_PARAMETER_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING: u32 =
        4006;
    pub const TYPE_PARAMETER_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE:
        u32 = 4008;
    pub const TYPE_PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVA:
        u32 = 4010;
    pub const TYPE_PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME:
        u32 = 4012;
    pub const TYPE_PARAMETER_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: u32 =
        4014;
    pub const TYPE_PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4016;
    pub const IMPLEMENTS_CLAUSE_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4019;
    pub const EXTENDS_CLAUSE_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4020;
    pub const EXTENDS_CLAUSE_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_2: u32 = 4021;
    pub const EXTENDS_CLAUSE_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4022;
    pub const EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NAMED: u32 =
        4023;
    pub const EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 = 4024;
    pub const EXPORTED_VARIABLE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4025;
    pub const PUBLIC_STATIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODU:
        u32 = 4026;
    pub const PUBLIC_STATIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODUL:
        u32 = 4027;
    pub const PUBLIC_STATIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4028;
    pub const PUBLIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT: u32 =
        4029;
    pub const PUBLIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 =
        4030;
    pub const PUBLIC_PROPERTY_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4031;
    pub const PROPERTY_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 = 4032;
    pub const PROPERTY_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4033;
    pub const PARAMETER_TYPE_OF_PUBLIC_STATIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME: u32 =
        4034;
    pub const PARAMETER_TYPE_OF_PUBLIC_STATIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVA:
        u32 = 4035;
    pub const PARAMETER_TYPE_OF_PUBLIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PR:
        u32 = 4036;
    pub const PARAMETER_TYPE_OF_PUBLIC_SETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME:
        u32 = 4037;
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO:
        u32 = 4038;
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO_2:
        u32 = 4039;
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE: u32 =
        4040;
    pub const RETURN_TYPE_OF_PUBLIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTER:
        u32 = 4041;
    pub const RETURN_TYPE_OF_PUBLIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVA:
        u32 = 4042;
    pub const RETURN_TYPE_OF_PUBLIC_GETTER_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 =
        4043;
    pub const RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAM:
        u32 = 4044;
    pub const RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRI:
        u32 = 4045;
    pub const RETURN_TYPE_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM: u32 =
        4046;
    pub const RETURN_TYPE_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NA:
        u32 = 4047;
    pub const RETURN_TYPE_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM:
        u32 = 4048;
    pub const RETURN_TYPE_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_N:
        u32 = 4049;
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO:
        u32 = 4050;
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FRO_2:
        u32 = 4051;
    pub const RETURN_TYPE_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE: u32 =
        4052;
    pub const RETURN_TYPE_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTER:
        u32 = 4053;
    pub const RETURN_TYPE_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVA:
        u32 = 4054;
    pub const RETURN_TYPE_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 =
        4055;
    pub const RETURN_TYPE_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE: u32 =
        4056;
    pub const RETURN_TYPE_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: u32 =
        4057;
    pub const RETURN_TYPE_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_C:
        u32 = 4058;
    pub const RETURN_TYPE_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 = 4059;
    pub const RETURN_TYPE_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4060;
    pub const PARAMETER_OF_CONSTRUCTOR_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL: u32 =
        4061;
    pub const PARAMETER_OF_CONSTRUCTOR_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_M:
        u32 = 4062;
    pub const PARAMETER_OF_CONSTRUCTOR_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4063;
    pub const PARAMETER_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME: u32 =
        4064;
    pub const PARAMETER_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVA:
        u32 = 4065;
    pub const PARAMETER_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PR:
        u32 = 4066;
    pub const PARAMETER_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME:
        u32 = 4067;
    pub const PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM: u32 =
        4068;
    pub const PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_2:
        u32 = 4069;
    pub const PARAMETER_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NA:
        u32 = 4070;
    pub const PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNA:
        u32 = 4071;
    pub const PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE:
        u32 = 4072;
    pub const PARAMETER_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 =
        4073;
    pub const PARAMETER_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MO:
        u32 = 4074;
    pub const PARAMETER_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4075;
    pub const PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CAN:
        u32 = 4076;
    pub const PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 = 4077;
    pub const PARAMETER_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4078;
    pub const EXPORTED_TYPE_ALIAS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4081;
    pub const DEFAULT_EXPORT_OF_THE_MODULE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4082;
    pub const TYPE_PARAMETER_OF_EXPORTED_TYPE_ALIAS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4083;
    pub const EXPORTED_TYPE_ALIAS_HAS_OR_IS_USING_PRIVATE_NAME_FROM_MODULE: u32 = 4084;
    pub const EXTENDS_CLAUSE_FOR_INFERRED_TYPE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4085;
    pub const PARAMETER_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_P:
        u32 = 4091;
    pub const PARAMETER_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAM:
        u32 = 4092;
    pub const PROPERTY_OF_EXPORTED_ANONYMOUS_CLASS_TYPE_MAY_NOT_BE_PRIVATE_OR_PROTECTED: u32 = 4094;
    pub const PUBLIC_STATIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE:
        u32 = 4095;
    pub const PUBLIC_STATIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 =
        4096;
    pub const PUBLIC_STATIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4097;
    pub const PUBLIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CA:
        u32 = 4098;
    pub const PUBLIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 = 4099;
    pub const PUBLIC_METHOD_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4100;
    pub const METHOD_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 = 4101;
    pub const METHOD_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4102;
    pub const TYPE_PARAMETER_OF_EXPORTED_MAPPED_OBJECT_TYPE_IS_USING_PRIVATE_NAME: u32 = 4103;
    pub const THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE: u32 = 4104;
    pub const PRIVATE_OR_PROTECTED_MEMBER_CANNOT_BE_ACCESSED_ON_A_TYPE_PARAMETER: u32 = 4105;
    pub const PARAMETER_OF_ACCESSOR_HAS_OR_IS_USING_PRIVATE_NAME: u32 = 4106;
    pub const PARAMETER_OF_ACCESSOR_HAS_OR_IS_USING_NAME_FROM_PRIVATE_MODULE: u32 = 4107;
    pub const PARAMETER_OF_ACCESSOR_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NA:
        u32 = 4108;
    pub const TYPE_ARGUMENTS_FOR_CIRCULARLY_REFERENCE_THEMSELVES: u32 = 4109;
    pub const TUPLE_TYPE_ARGUMENTS_CIRCULARLY_REFERENCE_THEMSELVES: u32 = 4110;
    pub const PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH: u32 = 4111;
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N:
        u32 = 4112;
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B:
        u32 = 4113;
    pub const THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE: u32 =
        4114;
    pub const THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME:
        u32 = 4115;
    pub const THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH:
        u32 = 4116;
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2:
        u32 = 4117;
    pub const THE_TYPE_OF_THIS_NODE_CANNOT_BE_SERIALIZED_BECAUSE_ITS_PROPERTY_CANNOT_BE_SERIAL:
        u32 = 4118;
    pub const THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES: u32 =
        4119;
    pub const THIS_PARAMETER_PROPERTY_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_I:
        u32 = 4120;
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_CONTAIN:
        u32 = 4121;
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D:
        u32 = 4122;
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D_2:
        u32 = 4123;
    pub const COMPILER_OPTION_OF_VALUE_IS_UNSTABLE_USE_NIGHTLY_TYPESCRIPT_TO_SILENCE_THIS_ERRO:
        u32 = 4124;
    pub const EACH_DECLARATION_OF_DIFFERS_IN_ITS_VALUE_WHERE_WAS_EXPECTED_BUT_WAS_GIVEN: u32 = 4125;
    pub const ONE_VALUE_OF_IS_THE_STRING_AND_THE_OTHER_IS_ASSUMED_TO_BE_AN_UNKNOWN_NUMERIC_VAL:
        u32 = 4126;
    pub const THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC: u32 = 4127;
    pub const THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_NAME_IS:
        u32 = 4128;
    pub const THE_CURRENT_HOST_DOES_NOT_SUPPORT_THE_OPTION: u32 = 5001;
    pub const CANNOT_FIND_THE_COMMON_SUBDIRECTORY_PATH_FOR_THE_INPUT_FILES: u32 = 5009;
    pub const FILE_SPECIFICATION_CANNOT_END_IN_A_RECURSIVE_DIRECTORY_WILDCARD: u32 = 5010;
    pub const THE_COMMON_SOURCE_DIRECTORY_OF_IS_THE_ROOTDIR_SETTING_MUST_BE_EXPLICITLY_SET_TO: u32 =
        5011;
    pub const CANNOT_READ_FILE: u32 = 5012;
    pub const UNKNOWN_COMPILER_OPTION: u32 = 5023;
    pub const COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE: u32 = 5024;
    pub const UNKNOWN_COMPILER_OPTION_DID_YOU_MEAN: u32 = 5025;
    pub const COULD_NOT_WRITE_FILE: u32 = 5033;
    pub const OPTION_PROJECT_CANNOT_BE_MIXED_WITH_SOURCE_FILES_ON_A_COMMAND_LINE: u32 = 5042;
    pub const OPTION_ISOLATEDMODULES_CAN_ONLY_BE_USED_WHEN_EITHER_OPTION_MODULE_IS_PROVIDED_OR:
        u32 = 5047;
    pub const OPTION_CAN_ONLY_BE_USED_WHEN_EITHER_OPTION_INLINESOURCEMAP_OR_OPTION_SOURCEMAP_I:
        u32 = 5051;
    pub const OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION: u32 = 5052;
    pub const OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION: u32 = 5053;
    pub const A_TSCONFIG_JSON_FILE_IS_ALREADY_DEFINED_AT: u32 = 5054;
    pub const CANNOT_WRITE_FILE_BECAUSE_IT_WOULD_OVERWRITE_INPUT_FILE: u32 = 5055;
    pub const CANNOT_WRITE_FILE_BECAUSE_IT_WOULD_BE_OVERWRITTEN_BY_MULTIPLE_INPUT_FILES: u32 = 5056;
    pub const CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY: u32 = 5057;
    pub const THE_SPECIFIED_PATH_DOES_NOT_EXIST: u32 = 5058;
    pub const INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER: u32 = 5059;
    pub const PATTERN_CAN_HAVE_AT_MOST_ONE_CHARACTER: u32 = 5061;
    pub const SUBSTITUTION_IN_PATTERN_CAN_HAVE_AT_MOST_ONE_CHARACTER: u32 = 5062;
    pub const SUBSTITUTIONS_FOR_PATTERN_SHOULD_BE_AN_ARRAY: u32 = 5063;
    pub const SUBSTITUTION_FOR_PATTERN_HAS_INCORRECT_TYPE_EXPECTED_STRING_GOT: u32 = 5064;
    pub const FILE_SPECIFICATION_CANNOT_CONTAIN_A_PARENT_DIRECTORY_THAT_APPEARS_AFTER_A_RECURS:
        u32 = 5065;
    pub const SUBSTITUTIONS_FOR_PATTERN_SHOULDNT_BE_AN_EMPTY_ARRAY: u32 = 5066;
    pub const INVALID_VALUE_FOR_JSXFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME: u32 = 5067;
    pub const ADDING_A_TSCONFIG_JSON_FILE_WILL_HELP_ORGANIZE_PROJECTS_THAT_CONTAIN_BOTH_TYPESC:
        u32 = 5068;
    pub const OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION: u32 = 5069;
    pub const OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULERESOLUTION_IS_SET_TO_CLA:
        u32 = 5070;
    pub const OPTION_RESOLVEJSONMODULE_CANNOT_BE_SPECIFIED_WHEN_MODULE_IS_SET_TO_NONE_SYSTEM_O:
        u32 = 5071;
    pub const UNKNOWN_BUILD_OPTION: u32 = 5072;
    pub const BUILD_OPTION_REQUIRES_A_VALUE_OF_TYPE: u32 = 5073;
    pub const OPTION_INCREMENTAL_CAN_ONLY_BE_SPECIFIED_USING_TSCONFIG_EMITTING_TO_SINGLE_FILE: u32 =
        5074;
    pub const IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE:
        u32 = 5075;
    pub const AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES: u32 = 5076;
    pub const UNKNOWN_BUILD_OPTION_DID_YOU_MEAN: u32 = 5077;
    pub const UNKNOWN_WATCH_OPTION: u32 = 5078;
    pub const UNKNOWN_WATCH_OPTION_DID_YOU_MEAN: u32 = 5079;
    pub const WATCH_OPTION_REQUIRES_A_VALUE_OF_TYPE: u32 = 5080;
    pub const CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_CURRENT_DIRECTORY: u32 = 5081;
    pub const COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO: u32 = 5082;
    pub const CANNOT_READ_FILE_2: u32 = 5083;
    pub const A_TUPLE_MEMBER_CANNOT_BE_BOTH_OPTIONAL_AND_REST: u32 = 5085;
    pub const A_LABELED_TUPLE_ELEMENT_IS_DECLARED_AS_OPTIONAL_WITH_A_QUESTION_MARK_AFTER_THE_N:
        u32 = 5086;
    pub const A_LABELED_TUPLE_ELEMENT_IS_DECLARED_AS_REST_WITH_A_BEFORE_THE_NAME_RATHER_THAN_B:
        u32 = 5087;
    pub const THE_INFERRED_TYPE_OF_REFERENCES_A_TYPE_WITH_A_CYCLIC_STRUCTURE_WHICH_CANNOT_BE_T:
        u32 = 5088;
    pub const OPTION_CANNOT_BE_SPECIFIED_WHEN_OPTION_JSX_IS: u32 = 5089;
    pub const NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD:
        u32 = 5090;
    pub const OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_IS_ENABLED: u32 = 5091;
    pub const THE_ROOT_VALUE_OF_A_FILE_MUST_BE_AN_OBJECT: u32 = 5092;
    pub const COMPILER_OPTION_MAY_ONLY_BE_USED_WITH_BUILD: u32 = 5093;
    pub const COMPILER_OPTION_MAY_NOT_BE_USED_WITH_BUILD: u32 = 5094;
    pub const OPTION_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LAT:
        u32 = 5095;
    pub const OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR:
        u32 = 5096;
    pub const AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS: u32 =
        5097;
    pub const OPTION_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDL:
        u32 = 5098;
    pub const OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT:
        u32 = 5101;
    pub const OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION: u32 = 5102;
    pub const INVALID_VALUE_FOR_IGNOREDEPRECATIONS: u32 = 5103;
    pub const OPTION_IS_REDUNDANT_AND_CANNOT_BE_SPECIFIED_WITH_OPTION: u32 = 5104;
    pub const OPTION_VERBATIMMODULESYNTAX_CANNOT_BE_USED_WHEN_MODULE_IS_SET_TO_UMD_AMD_OR_SYST:
        u32 = 5105;
    pub const USE_INSTEAD: u32 = 5106;
    pub const OPTION_IS_DEPRECATED_AND_WILL_STOP_FUNCTIONING_IN_TYPESCRIPT_SPECIFY_COMPILEROPT_2:
        u32 = 5107;
    pub const OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION_2: u32 = 5108;
    pub const OPTION_MODULERESOLUTION_MUST_BE_SET_TO_OR_LEFT_UNSPECIFIED_WHEN_OPTION_MODULE_IS:
        u32 = 5109;
    pub const OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO: u32 = 5110;
    pub const VISIT_HTTPS_AKA_MS_TS6_FOR_MIGRATION_INFORMATION: u32 = 5111;
    pub const TSCONFIG_JSON_IS_PRESENT_BUT_WILL_NOT_BE_LOADED_IF_FILES_ARE_SPECIFIED_ON_COMMAN:
        u32 = 5112;
    pub const GENERATES_A_SOURCEMAP_FOR_EACH_CORRESPONDING_D_TS_FILE: u32 = 6000;
    pub const CONCATENATE_AND_EMIT_OUTPUT_TO_SINGLE_FILE: u32 = 6001;
    pub const GENERATES_CORRESPONDING_D_TS_FILE: u32 = 6002;
    pub const SPECIFY_THE_LOCATION_WHERE_DEBUGGER_SHOULD_LOCATE_TYPESCRIPT_FILES_INSTEAD_OF_SO:
        u32 = 6004;
    pub const WATCH_INPUT_FILES: u32 = 6005;
    pub const REDIRECT_OUTPUT_STRUCTURE_TO_THE_DIRECTORY: u32 = 6006;
    pub const DO_NOT_ERASE_CONST_ENUM_DECLARATIONS_IN_GENERATED_CODE: u32 = 6007;
    pub const DO_NOT_EMIT_OUTPUTS_IF_ANY_ERRORS_WERE_REPORTED: u32 = 6008;
    pub const DO_NOT_EMIT_COMMENTS_TO_OUTPUT: u32 = 6009;
    pub const DO_NOT_EMIT_OUTPUTS: u32 = 6010;
    pub const ALLOW_DEFAULT_IMPORTS_FROM_MODULES_WITH_NO_DEFAULT_EXPORT_THIS_DOES_NOT_AFFECT_C:
        u32 = 6011;
    pub const SKIP_TYPE_CHECKING_OF_DECLARATION_FILES: u32 = 6012;
    pub const DO_NOT_RESOLVE_THE_REAL_PATH_OF_SYMLINKS: u32 = 6013;
    pub const ONLY_EMIT_D_TS_DECLARATION_FILES: u32 = 6014;
    pub const SPECIFY_ECMASCRIPT_TARGET_VERSION: u32 = 6015;
    pub const SPECIFY_MODULE_CODE_GENERATION: u32 = 6016;
    pub const PRINT_THIS_MESSAGE: u32 = 6017;
    pub const PRINT_THE_COMPILERS_VERSION: u32 = 6019;
    pub const COMPILE_THE_PROJECT_GIVEN_THE_PATH_TO_ITS_CONFIGURATION_FILE_OR_TO_A_FOLDER_WITH:
        u32 = 6020;
    pub const SYNTAX: u32 = 6023;
    pub const OPTIONS: u32 = 6024;
    pub const FILE: u32 = 6025;
    pub const EXAMPLES: u32 = 6026;
    pub const OPTIONS_2: u32 = 6027;
    pub const VERSION: u32 = 6029;
    pub const INSERT_COMMAND_LINE_OPTIONS_AND_FILES_FROM_A_FILE: u32 = 6030;
    pub const STARTING_COMPILATION_IN_WATCH_MODE: u32 = 6031;
    pub const FILE_CHANGE_DETECTED_STARTING_INCREMENTAL_COMPILATION: u32 = 6032;
    pub const KIND: u32 = 6034;
    pub const FILE_2: u32 = 6035;
    pub const VERSION_2: u32 = 6036;
    pub const LOCATION: u32 = 6037;
    pub const DIRECTORY: u32 = 6038;
    pub const STRATEGY: u32 = 6039;
    pub const FILE_OR_DIRECTORY: u32 = 6040;
    pub const ERRORS_FILES: u32 = 6041;
    pub const GENERATES_CORRESPONDING_MAP_FILE: u32 = 6043;
    pub const COMPILER_OPTION_EXPECTS_AN_ARGUMENT: u32 = 6044;
    pub const UNTERMINATED_QUOTED_STRING_IN_RESPONSE_FILE: u32 = 6045;
    pub const ARGUMENT_FOR_OPTION_MUST_BE: u32 = 6046;
    pub const LOCALE_MUST_BE_OF_THE_FORM_LANGUAGE_OR_LANGUAGE_TERRITORY_FOR_EXAMPLE_OR: u32 = 6048;
    pub const UNABLE_TO_OPEN_FILE: u32 = 6050;
    pub const CORRUPTED_LOCALE_FILE: u32 = 6051;
    pub const RAISE_ERROR_ON_EXPRESSIONS_AND_DECLARATIONS_WITH_AN_IMPLIED_ANY_TYPE: u32 = 6052;
    pub const FILE_NOT_FOUND: u32 = 6053;
    pub const FILE_HAS_AN_UNSUPPORTED_EXTENSION_THE_ONLY_SUPPORTED_EXTENSIONS_ARE: u32 = 6054;
    pub const SUPPRESS_NOIMPLICITANY_ERRORS_FOR_INDEXING_OBJECTS_LACKING_INDEX_SIGNATURES: u32 =
        6055;
    pub const DO_NOT_EMIT_DECLARATIONS_FOR_CODE_THAT_HAS_AN_INTERNAL_ANNOTATION: u32 = 6056;
    pub const SPECIFY_THE_ROOT_DIRECTORY_OF_INPUT_FILES_USE_TO_CONTROL_THE_OUTPUT_DIRECTORY_ST:
        u32 = 6058;
    pub const FILE_IS_NOT_UNDER_ROOTDIR_ROOTDIR_IS_EXPECTED_TO_CONTAIN_ALL_SOURCE_FILES: u32 = 6059;
    pub const SPECIFY_THE_END_OF_LINE_SEQUENCE_TO_BE_USED_WHEN_EMITTING_FILES_CRLF_DOS_OR_LF_U:
        u32 = 6060;
    pub const NEWLINE: u32 = 6061;
    pub const OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN:
        u32 = 6064;
    pub const ENABLES_EXPERIMENTAL_SUPPORT_FOR_ES7_DECORATORS: u32 = 6065;
    pub const ENABLES_EXPERIMENTAL_SUPPORT_FOR_EMITTING_TYPE_METADATA_FOR_DECORATORS: u32 = 6066;
    pub const INITIALIZES_A_TYPESCRIPT_PROJECT_AND_CREATES_A_TSCONFIG_JSON_FILE: u32 = 6070;
    pub const SUCCESSFULLY_CREATED_A_TSCONFIG_JSON_FILE: u32 = 6071;
    pub const SUPPRESS_EXCESS_PROPERTY_CHECKS_FOR_OBJECT_LITERALS: u32 = 6072;
    pub const STYLIZE_ERRORS_AND_MESSAGES_USING_COLOR_AND_CONTEXT_EXPERIMENTAL: u32 = 6073;
    pub const DO_NOT_REPORT_ERRORS_ON_UNUSED_LABELS: u32 = 6074;
    pub const REPORT_ERROR_WHEN_NOT_ALL_CODE_PATHS_IN_FUNCTION_RETURN_A_VALUE: u32 = 6075;
    pub const REPORT_ERRORS_FOR_FALLTHROUGH_CASES_IN_SWITCH_STATEMENT: u32 = 6076;
    pub const DO_NOT_REPORT_ERRORS_ON_UNREACHABLE_CODE: u32 = 6077;
    pub const DISALLOW_INCONSISTENTLY_CASED_REFERENCES_TO_THE_SAME_FILE: u32 = 6078;
    pub const SPECIFY_LIBRARY_FILES_TO_BE_INCLUDED_IN_THE_COMPILATION: u32 = 6079;
    pub const SPECIFY_JSX_CODE_GENERATION: u32 = 6080;
    pub const ONLY_AMD_AND_SYSTEM_MODULES_ARE_SUPPORTED_ALONGSIDE: u32 = 6082;
    pub const BASE_DIRECTORY_TO_RESOLVE_NON_ABSOLUTE_MODULE_NAMES: u32 = 6083;
    pub const DEPRECATED_USE_JSXFACTORY_INSTEAD_SPECIFY_THE_OBJECT_INVOKED_FOR_CREATEELEMENT_W:
        u32 = 6084;
    pub const ENABLE_TRACING_OF_THE_NAME_RESOLUTION_PROCESS: u32 = 6085;
    pub const RESOLVING_MODULE_FROM: u32 = 6086;
    pub const EXPLICITLY_SPECIFIED_MODULE_RESOLUTION_KIND: u32 = 6087;
    pub const MODULE_RESOLUTION_KIND_IS_NOT_SPECIFIED_USING: u32 = 6088;
    pub const MODULE_NAME_WAS_SUCCESSFULLY_RESOLVED_TO: u32 = 6089;
    pub const MODULE_NAME_WAS_NOT_RESOLVED: u32 = 6090;
    pub const PATHS_OPTION_IS_SPECIFIED_LOOKING_FOR_A_PATTERN_TO_MATCH_MODULE_NAME: u32 = 6091;
    pub const MODULE_NAME_MATCHED_PATTERN: u32 = 6092;
    pub const TRYING_SUBSTITUTION_CANDIDATE_MODULE_LOCATION: u32 = 6093;
    pub const RESOLVING_MODULE_NAME_RELATIVE_TO_BASE_URL: u32 = 6094;
    pub const LOADING_MODULE_AS_FILE_FOLDER_CANDIDATE_MODULE_LOCATION_TARGET_FILE_TYPES: u32 = 6095;
    pub const FILE_DOES_NOT_EXIST: u32 = 6096;
    pub const FILE_EXISTS_USE_IT_AS_A_NAME_RESOLUTION_RESULT: u32 = 6097;
    pub const LOADING_MODULE_FROM_NODE_MODULES_FOLDER_TARGET_FILE_TYPES: u32 = 6098;
    pub const FOUND_PACKAGE_JSON_AT: u32 = 6099;
    pub const PACKAGE_JSON_DOES_NOT_HAVE_A_FIELD: u32 = 6100;
    pub const PACKAGE_JSON_HAS_FIELD_THAT_REFERENCES: u32 = 6101;
    pub const ALLOW_JAVASCRIPT_FILES_TO_BE_COMPILED: u32 = 6102;
    pub const CHECKING_IF_IS_THE_LONGEST_MATCHING_PREFIX_FOR: u32 = 6104;
    pub const EXPECTED_TYPE_OF_FIELD_IN_PACKAGE_JSON_TO_BE_GOT: u32 = 6105;
    pub const BASEURL_OPTION_IS_SET_TO_USING_THIS_VALUE_TO_RESOLVE_NON_RELATIVE_MODULE_NAME: u32 =
        6106;
    pub const ROOTDIRS_OPTION_IS_SET_USING_IT_TO_RESOLVE_RELATIVE_MODULE_NAME: u32 = 6107;
    pub const LONGEST_MATCHING_PREFIX_FOR_IS: u32 = 6108;
    pub const LOADING_FROM_THE_ROOT_DIR_CANDIDATE_LOCATION: u32 = 6109;
    pub const TRYING_OTHER_ENTRIES_IN_ROOTDIRS: u32 = 6110;
    pub const MODULE_RESOLUTION_USING_ROOTDIRS_HAS_FAILED: u32 = 6111;
    pub const DO_NOT_EMIT_USE_STRICT_DIRECTIVES_IN_MODULE_OUTPUT: u32 = 6112;
    pub const ENABLE_STRICT_NULL_CHECKS: u32 = 6113;
    pub const UNKNOWN_OPTION_EXCLUDES_DID_YOU_MEAN_EXCLUDE: u32 = 6114;
    pub const RAISE_ERROR_ON_THIS_EXPRESSIONS_WITH_AN_IMPLIED_ANY_TYPE: u32 = 6115;
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_ROOT_DIRECTORY: u32 = 6116;
    pub const TYPE_REFERENCE_DIRECTIVE_WAS_SUCCESSFULLY_RESOLVED_TO_PRIMARY: u32 = 6119;
    pub const TYPE_REFERENCE_DIRECTIVE_WAS_NOT_RESOLVED: u32 = 6120;
    pub const RESOLVING_WITH_PRIMARY_SEARCH_PATH: u32 = 6121;
    pub const ROOT_DIRECTORY_CANNOT_BE_DETERMINED_SKIPPING_PRIMARY_SEARCH_PATHS: u32 = 6122;
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_ROOT_DIRECTORY_NOT_SET: u32 = 6123;
    pub const TYPE_DECLARATION_FILES_TO_BE_INCLUDED_IN_COMPILATION: u32 = 6124;
    pub const LOOKING_UP_IN_NODE_MODULES_FOLDER_INITIAL_LOCATION: u32 = 6125;
    pub const CONTAINING_FILE_IS_NOT_SPECIFIED_AND_ROOT_DIRECTORY_CANNOT_BE_DETERMINED_SKIPPIN:
        u32 = 6126;
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_NOT_SET_ROOT_DIRECTORY: u32 = 6127;
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE_NOT_SET_ROOT_DIRECTORY_NOT_SE:
        u32 = 6128;
    pub const RESOLVING_REAL_PATH_FOR_RESULT: u32 = 6130;
    pub const CANNOT_COMPILE_MODULES_USING_OPTION_UNLESS_THE_MODULE_FLAG_IS_AMD_OR_SYSTEM: u32 =
        6131;
    pub const FILE_NAME_HAS_A_EXTENSION_STRIPPING_IT: u32 = 6132;
    pub const IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ: u32 = 6133;
    pub const REPORT_ERRORS_ON_UNUSED_LOCALS: u32 = 6134;
    pub const REPORT_ERRORS_ON_UNUSED_PARAMETERS: u32 = 6135;
    pub const THE_MAXIMUM_DEPENDENCY_DEPTH_TO_SEARCH_UNDER_NODE_MODULES_AND_LOAD_JAVASCRIPT_FI:
        u32 = 6136;
    pub const CANNOT_IMPORT_TYPE_DECLARATION_FILES_CONSIDER_IMPORTING_INSTEAD_OF: u32 = 6137;
    pub const PROPERTY_IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ: u32 = 6138;
    pub const IMPORT_EMIT_HELPERS_FROM_TSLIB: u32 = 6139;
    pub const AUTO_DISCOVERY_FOR_TYPINGS_IS_ENABLED_IN_PROJECT_RUNNING_EXTRA_RESOLUTION_PASS_F:
        u32 = 6140;
    pub const PARSE_IN_STRICT_MODE_AND_EMIT_USE_STRICT_FOR_EACH_SOURCE_FILE: u32 = 6141;
    pub const MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET: u32 = 6142;
    pub const MODULE_WAS_RESOLVED_AS_LOCALLY_DECLARED_AMBIENT_MODULE_IN_FILE: u32 = 6144;
    pub const SPECIFY_THE_JSX_FACTORY_FUNCTION_TO_USE_WHEN_TARGETING_REACT_JSX_EMIT_E_G_REACT: u32 =
        6146;
    pub const RESOLUTION_FOR_MODULE_WAS_FOUND_IN_CACHE_FROM_LOCATION: u32 = 6147;
    pub const DIRECTORY_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT: u32 = 6148;
    pub const SHOW_DIAGNOSTIC_INFORMATION: u32 = 6149;
    pub const SHOW_VERBOSE_DIAGNOSTIC_INFORMATION: u32 = 6150;
    pub const EMIT_A_SINGLE_FILE_WITH_SOURCE_MAPS_INSTEAD_OF_HAVING_A_SEPARATE_FILE: u32 = 6151;
    pub const EMIT_THE_SOURCE_ALONGSIDE_THE_SOURCEMAPS_WITHIN_A_SINGLE_FILE_REQUIRES_INLINESOU:
        u32 = 6152;
    pub const TRANSPILE_EACH_FILE_AS_A_SEPARATE_MODULE_SIMILAR_TO_TS_TRANSPILEMODULE: u32 = 6153;
    pub const PRINT_NAMES_OF_GENERATED_FILES_PART_OF_THE_COMPILATION: u32 = 6154;
    pub const PRINT_NAMES_OF_FILES_PART_OF_THE_COMPILATION: u32 = 6155;
    pub const THE_LOCALE_USED_WHEN_DISPLAYING_MESSAGES_TO_THE_USER_E_G_EN_US: u32 = 6156;
    pub const DO_NOT_GENERATE_CUSTOM_HELPER_FUNCTIONS_LIKE_EXTENDS_IN_COMPILED_OUTPUT: u32 = 6157;
    pub const DO_NOT_INCLUDE_THE_DEFAULT_LIBRARY_FILE_LIB_D_TS: u32 = 6158;
    pub const DO_NOT_ADD_TRIPLE_SLASH_REFERENCES_OR_IMPORTED_MODULES_TO_THE_LIST_OF_COMPILED_F:
        u32 = 6159;
    pub const DEPRECATED_USE_SKIPLIBCHECK_INSTEAD_SKIP_TYPE_CHECKING_OF_DEFAULT_LIBRARY_DECLAR:
        u32 = 6160;
    pub const LIST_OF_FOLDERS_TO_INCLUDE_TYPE_DEFINITIONS_FROM: u32 = 6161;
    pub const DISABLE_SIZE_LIMITATIONS_ON_JAVASCRIPT_PROJECTS: u32 = 6162;
    pub const THE_CHARACTER_SET_OF_THE_INPUT_FILES: u32 = 6163;
    pub const SKIPPING_MODULE_THAT_LOOKS_LIKE_AN_ABSOLUTE_URI_TARGET_FILE_TYPES: u32 = 6164;
    pub const DO_NOT_TRUNCATE_ERROR_MESSAGES: u32 = 6165;
    pub const OUTPUT_DIRECTORY_FOR_GENERATED_DECLARATION_FILES: u32 = 6166;
    pub const A_SERIES_OF_ENTRIES_WHICH_RE_MAP_IMPORTS_TO_LOOKUP_LOCATIONS_RELATIVE_TO_THE_BAS:
        u32 = 6167;
    pub const LIST_OF_ROOT_FOLDERS_WHOSE_COMBINED_CONTENT_REPRESENTS_THE_STRUCTURE_OF_THE_PROJ:
        u32 = 6168;
    pub const SHOW_ALL_COMPILER_OPTIONS: u32 = 6169;
    pub const DEPRECATED_USE_OUTFILE_INSTEAD_CONCATENATE_AND_EMIT_OUTPUT_TO_SINGLE_FILE: u32 = 6170;
    pub const COMMAND_LINE_OPTIONS: u32 = 6171;
    pub const PROVIDE_FULL_SUPPORT_FOR_ITERABLES_IN_FOR_OF_SPREAD_AND_DESTRUCTURING_WHEN_TARGE:
        u32 = 6179;
    pub const ENABLE_ALL_STRICT_TYPE_CHECKING_OPTIONS: u32 = 6180;
    pub const SCOPED_PACKAGE_DETECTED_LOOKING_IN: u32 = 6182;
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCESSFULLY_RESOLVED_TO:
        u32 = 6183;
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCESSFULLY_RESOLVED_TO_2:
        u32 = 6184;
    pub const ENABLE_STRICT_CHECKING_OF_FUNCTION_TYPES: u32 = 6186;
    pub const ENABLE_STRICT_CHECKING_OF_PROPERTY_INITIALIZATION_IN_CLASSES: u32 = 6187;
    pub const NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE: u32 = 6188;
    pub const MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED: u32 = 6189;
    pub const WHETHER_TO_KEEP_OUTDATED_CONSOLE_OUTPUT_IN_WATCH_MODE_INSTEAD_OF_CLEARING_THE_SC:
        u32 = 6191;
    pub const ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED: u32 = 6192;
    pub const FOUND_1_ERROR_WATCHING_FOR_FILE_CHANGES: u32 = 6193;
    pub const FOUND_ERRORS_WATCHING_FOR_FILE_CHANGES: u32 = 6194;
    pub const RESOLVE_KEYOF_TO_STRING_VALUED_PROPERTY_NAMES_ONLY_NO_NUMBERS_OR_SYMBOLS: u32 = 6195;
    pub const IS_DECLARED_BUT_NEVER_USED: u32 = 6196;
    pub const INCLUDE_MODULES_IMPORTED_WITH_JSON_EXTENSION: u32 = 6197;
    pub const ALL_DESTRUCTURED_ELEMENTS_ARE_UNUSED: u32 = 6198;
    pub const ALL_VARIABLES_ARE_UNUSED: u32 = 6199;
    pub const DEFINITIONS_OF_THE_FOLLOWING_IDENTIFIERS_CONFLICT_WITH_THOSE_IN_ANOTHER_FILE: u32 =
        6200;
    pub const CONFLICTS_ARE_IN_THIS_FILE: u32 = 6201;
    pub const PROJECT_REFERENCES_MAY_NOT_FORM_A_CIRCULAR_GRAPH_CYCLE_DETECTED: u32 = 6202;
    pub const WAS_ALSO_DECLARED_HERE: u32 = 6203;
    pub const AND_HERE: u32 = 6204;
    pub const ALL_TYPE_PARAMETERS_ARE_UNUSED: u32 = 6205;
    pub const PACKAGE_JSON_HAS_A_TYPESVERSIONS_FIELD_WITH_VERSION_SPECIFIC_PATH_MAPPINGS: u32 =
        6206;
    pub const PACKAGE_JSON_DOES_NOT_HAVE_A_TYPESVERSIONS_ENTRY_THAT_MATCHES_VERSION: u32 = 6207;
    pub const PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_THAT_MATCHES_COMPILER_VERSION_LOOKING_FOR:
        u32 = 6208;
    pub const PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_THAT_IS_NOT_A_VALID_SEMVER_RANGE: u32 = 6209;
    pub const AN_ARGUMENT_FOR_WAS_NOT_PROVIDED: u32 = 6210;
    pub const AN_ARGUMENT_MATCHING_THIS_BINDING_PATTERN_WAS_NOT_PROVIDED: u32 = 6211;
    pub const DID_YOU_MEAN_TO_CALL_THIS_EXPRESSION: u32 = 6212;
    pub const DID_YOU_MEAN_TO_USE_NEW_WITH_THIS_EXPRESSION: u32 = 6213;
    pub const ENABLE_STRICT_BIND_CALL_AND_APPLY_METHODS_ON_FUNCTIONS: u32 = 6214;
    pub const USING_COMPILER_OPTIONS_OF_PROJECT_REFERENCE_REDIRECT: u32 = 6215;
    pub const FOUND_1_ERROR: u32 = 6216;
    pub const FOUND_ERRORS: u32 = 6217;
    pub const MODULE_NAME_WAS_SUCCESSFULLY_RESOLVED_TO_WITH_PACKAGE_ID: u32 = 6218;
    pub const TYPE_REFERENCE_DIRECTIVE_WAS_SUCCESSFULLY_RESOLVED_TO_WITH_PACKAGE_ID_PRIMARY: u32 =
        6219;
    pub const PACKAGE_JSON_HAD_A_FALSY_FIELD: u32 = 6220;
    pub const DISABLE_USE_OF_SOURCE_FILES_INSTEAD_OF_DECLARATION_FILES_FROM_REFERENCED_PROJECT:
        u32 = 6221;
    pub const EMIT_CLASS_FIELDS_WITH_DEFINE_INSTEAD_OF_SET: u32 = 6222;
    pub const GENERATES_A_CPU_PROFILE: u32 = 6223;
    pub const DISABLE_SOLUTION_SEARCHING_FOR_THIS_PROJECT: u32 = 6224;
    pub const SPECIFY_STRATEGY_FOR_WATCHING_FILE_FIXEDPOLLINGINTERVAL_DEFAULT_PRIORITYPOLLINGI:
        u32 = 6225;
    pub const SPECIFY_STRATEGY_FOR_WATCHING_DIRECTORY_ON_PLATFORMS_THAT_DONT_SUPPORT_RECURSIVE:
        u32 = 6226;
    pub const SPECIFY_STRATEGY_FOR_CREATING_A_POLLING_WATCH_WHEN_IT_FAILS_TO_CREATE_USING_FILE:
        u32 = 6227;
    pub const TAG_EXPECTS_AT_LEAST_ARGUMENTS_BUT_THE_JSX_FACTORY_PROVIDES_AT_MOST: u32 = 6229;
    pub const OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_FALSE_OR_NULL_ON_CO:
        u32 = 6230;
    pub const COULD_NOT_RESOLVE_THE_PATH_WITH_THE_EXTENSIONS: u32 = 6231;
    pub const DECLARATION_AUGMENTS_DECLARATION_IN_ANOTHER_FILE_THIS_CANNOT_BE_SERIALIZED: u32 =
        6232;
    pub const THIS_IS_THE_DECLARATION_BEING_AUGMENTED_CONSIDER_MOVING_THE_AUGMENTING_DECLARATI:
        u32 = 6233;
    pub const THIS_EXPRESSION_IS_NOT_CALLABLE_BECAUSE_IT_IS_A_GET_ACCESSOR_DID_YOU_MEAN_TO_USE:
        u32 = 6234;
    pub const DISABLE_LOADING_REFERENCED_PROJECTS: u32 = 6235;
    pub const ARGUMENTS_FOR_THE_REST_PARAMETER_WERE_NOT_PROVIDED: u32 = 6236;
    pub const GENERATES_AN_EVENT_TRACE_AND_A_LIST_OF_TYPES: u32 = 6237;
    pub const SPECIFY_THE_MODULE_SPECIFIER_TO_BE_USED_TO_IMPORT_THE_JSX_AND_JSXS_FACTORY_FUNCT:
        u32 = 6238;
    pub const FILE_EXISTS_ACCORDING_TO_EARLIER_CACHED_LOOKUPS: u32 = 6239;
    pub const FILE_DOES_NOT_EXIST_ACCORDING_TO_EARLIER_CACHED_LOOKUPS: u32 = 6240;
    pub const RESOLUTION_FOR_TYPE_REFERENCE_DIRECTIVE_WAS_FOUND_IN_CACHE_FROM_LOCATION: u32 = 6241;
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_CONTAINING_FILE: u32 = 6242;
    pub const INTERPRET_OPTIONAL_PROPERTY_TYPES_AS_WRITTEN_RATHER_THAN_ADDING_UNDEFINED: u32 = 6243;
    pub const MODULES: u32 = 6244;
    pub const FILE_MANAGEMENT: u32 = 6245;
    pub const EMIT: u32 = 6246;
    pub const JAVASCRIPT_SUPPORT: u32 = 6247;
    pub const TYPE_CHECKING: u32 = 6248;
    pub const EDITOR_SUPPORT: u32 = 6249;
    pub const WATCH_AND_BUILD_MODES: u32 = 6250;
    pub const COMPILER_DIAGNOSTICS: u32 = 6251;
    pub const INTEROP_CONSTRAINTS: u32 = 6252;
    pub const BACKWARDS_COMPATIBILITY: u32 = 6253;
    pub const LANGUAGE_AND_ENVIRONMENT: u32 = 6254;
    pub const PROJECTS: u32 = 6255;
    pub const OUTPUT_FORMATTING: u32 = 6256;
    pub const COMPLETENESS: u32 = 6257;
    pub const SHOULD_BE_SET_INSIDE_THE_COMPILEROPTIONS_OBJECT_OF_THE_CONFIG_JSON_FILE: u32 = 6258;
    pub const FOUND_1_ERROR_IN: u32 = 6259;
    pub const FOUND_ERRORS_IN_THE_SAME_FILE_STARTING_AT: u32 = 6260;
    pub const FOUND_ERRORS_IN_FILES: u32 = 6261;
    pub const FILE_NAME_HAS_A_EXTENSION_LOOKING_UP_INSTEAD: u32 = 6262;
    pub const MODULE_WAS_RESOLVED_TO_BUT_ALLOWARBITRARYEXTENSIONS_IS_NOT_SET: u32 = 6263;
    pub const ENABLE_IMPORTING_FILES_WITH_ANY_EXTENSION_PROVIDED_A_DECLARATION_FILE_IS_PRESENT:
        u32 = 6264;
    pub const RESOLVING_TYPE_REFERENCE_DIRECTIVE_FOR_PROGRAM_THAT_SPECIFIES_CUSTOM_TYPEROOTS_S:
        u32 = 6265;
    pub const OPTION_CAN_ONLY_BE_SPECIFIED_ON_COMMAND_LINE: u32 = 6266;
    pub const DIRECTORY_HAS_NO_CONTAINING_PACKAGE_JSON_SCOPE_IMPORTS_WILL_NOT_RESOLVE: u32 = 6270;
    pub const IMPORT_SPECIFIER_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH: u32 = 6271;
    pub const INVALID_IMPORT_SPECIFIER_HAS_NO_POSSIBLE_RESOLUTIONS: u32 = 6272;
    pub const PACKAGE_JSON_SCOPE_HAS_NO_IMPORTS_DEFINED: u32 = 6273;
    pub const PACKAGE_JSON_SCOPE_EXPLICITLY_MAPS_SPECIFIER_TO_NULL: u32 = 6274;
    pub const PACKAGE_JSON_SCOPE_HAS_INVALID_TYPE_FOR_TARGET_OF_SPECIFIER: u32 = 6275;
    pub const EXPORT_SPECIFIER_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH: u32 = 6276;
    pub const RESOLUTION_OF_NON_RELATIVE_NAME_FAILED_TRYING_WITH_MODERN_NODE_RESOLUTION_FEATUR:
        u32 = 6277;
    pub const THERE_ARE_TYPES_AT_BUT_THIS_RESULT_COULD_NOT_BE_RESOLVED_WHEN_RESPECTING_PACKAGE:
        u32 = 6278;
    pub const RESOLUTION_OF_NON_RELATIVE_NAME_FAILED_TRYING_WITH_MODULERESOLUTION_BUNDLER_TO_S:
        u32 = 6279;
    pub const THERE_ARE_TYPES_AT_BUT_THIS_RESULT_COULD_NOT_BE_RESOLVED_UNDER_YOUR_CURRENT_MODU:
        u32 = 6280;
    pub const PACKAGE_JSON_HAS_A_PEERDEPENDENCIES_FIELD: u32 = 6281;
    pub const FOUND_PEERDEPENDENCY_WITH_VERSION: u32 = 6282;
    pub const FAILED_TO_FIND_PEERDEPENDENCY: u32 = 6283;
    pub const FILE_LAYOUT: u32 = 6284;
    pub const ENVIRONMENT_SETTINGS: u32 = 6285;
    pub const SEE_ALSO_HTTPS_AKA_MS_TSCONFIG_MODULE: u32 = 6286;
    pub const FOR_NODEJS: u32 = 6287;
    pub const AND_NPM_INSTALL_D_TYPES_NODE: u32 = 6290;
    pub const OTHER_OUTPUTS: u32 = 6291;
    pub const STRICTER_TYPECHECKING_OPTIONS: u32 = 6292;
    pub const STYLE_OPTIONS: u32 = 6293;
    pub const RECOMMENDED_OPTIONS: u32 = 6294;
    pub const ENABLE_PROJECT_COMPILATION: u32 = 6302;
    pub const COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT: u32 = 6304;
    pub const OUTPUT_FILE_HAS_NOT_BEEN_BUILT_FROM_SOURCE_FILE: u32 = 6305;
    pub const REFERENCED_PROJECT_MUST_HAVE_SETTING_COMPOSITE_TRUE: u32 = 6306;
    pub const FILE_IS_NOT_LISTED_WITHIN_THE_FILE_LIST_OF_PROJECT_PROJECTS_MUST_LIST_ALL_FILES: u32 =
        6307;
    pub const REFERENCED_PROJECT_MAY_NOT_DISABLE_EMIT: u32 = 6310;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_OUTPUT_IS_OLDER_THAN_INPUT: u32 = 6350;
    pub const PROJECT_IS_UP_TO_DATE_BECAUSE_NEWEST_INPUT_IS_OLDER_THAN_OUTPUT: u32 = 6351;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_OUTPUT_FILE_DOES_NOT_EXIST: u32 = 6352;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_ITS_DEPENDENCY_IS_OUT_OF_DATE: u32 = 6353;
    pub const PROJECT_IS_UP_TO_DATE_WITH_D_TS_FILES_FROM_ITS_DEPENDENCIES: u32 = 6354;
    pub const PROJECTS_IN_THIS_BUILD: u32 = 6355;
    pub const A_NON_DRY_BUILD_WOULD_DELETE_THE_FOLLOWING_FILES: u32 = 6356;
    pub const A_NON_DRY_BUILD_WOULD_BUILD_PROJECT: u32 = 6357;
    pub const BUILDING_PROJECT: u32 = 6358;
    pub const UPDATING_OUTPUT_TIMESTAMPS_OF_PROJECT: u32 = 6359;
    pub const PROJECT_IS_UP_TO_DATE: u32 = 6361;
    pub const SKIPPING_BUILD_OF_PROJECT_BECAUSE_ITS_DEPENDENCY_HAS_ERRORS: u32 = 6362;
    pub const PROJECT_CANT_BE_BUILT_BECAUSE_ITS_DEPENDENCY_HAS_ERRORS: u32 = 6363;
    pub const BUILD_ONE_OR_MORE_PROJECTS_AND_THEIR_DEPENDENCIES_IF_OUT_OF_DATE: u32 = 6364;
    pub const DELETE_THE_OUTPUTS_OF_ALL_PROJECTS: u32 = 6365;
    pub const SHOW_WHAT_WOULD_BE_BUILT_OR_DELETED_IF_SPECIFIED_WITH_CLEAN: u32 = 6367;
    pub const OPTION_BUILD_MUST_BE_THE_FIRST_COMMAND_LINE_ARGUMENT: u32 = 6369;
    pub const OPTIONS_AND_CANNOT_BE_COMBINED: u32 = 6370;
    pub const UPDATING_UNCHANGED_OUTPUT_TIMESTAMPS_OF_PROJECT: u32 = 6371;
    pub const A_NON_DRY_BUILD_WOULD_UPDATE_TIMESTAMPS_FOR_OUTPUT_OF_PROJECT: u32 = 6374;
    pub const CANNOT_WRITE_FILE_BECAUSE_IT_WILL_OVERWRITE_TSBUILDINFO_FILE_GENERATED_BY_REFERE:
        u32 = 6377;
    pub const COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION: u32 = 6379;
    pub const SPECIFY_FILE_TO_STORE_INCREMENTAL_COMPILATION_INFORMATION: u32 = 6380;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_OUTPUT_FOR_IT_WAS_GENERATED_WITH_VERSION_THAT_DIF:
        u32 = 6381;
    pub const SKIPPING_BUILD_OF_PROJECT_BECAUSE_ITS_DEPENDENCY_WAS_NOT_BUILT: u32 = 6382;
    pub const PROJECT_CANT_BE_BUILT_BECAUSE_ITS_DEPENDENCY_WAS_NOT_BUILT: u32 = 6383;
    pub const HAVE_RECOMPILES_IN_INCREMENTAL_AND_WATCH_ASSUME_THAT_CHANGES_WITHIN_A_FILE_WILL: u32 =
        6384;
    pub const IS_DEPRECATED: u32 = 6385;
    pub const PERFORMANCE_TIMINGS_FOR_DIAGNOSTICS_OR_EXTENDEDDIAGNOSTICS_ARE_NOT_AVAILABLE_IN: u32 =
        6386;
    pub const THE_SIGNATURE_OF_IS_DEPRECATED: u32 = 6387;
    pub const PROJECT_IS_BEING_FORCIBLY_REBUILT: u32 = 6388;
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_OF_OLD_PROGRAM_IT_WAS_NOT_RESOLVED: u32 = 6389;
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCES:
        u32 = 6390;
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_OF_OLD_PROGRAM_IT_WAS_SUCCES_2:
        u32 = 6391;
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_OF_OLD_PROGRAM_IT_WAS_NOT_RE:
        u32 = 6392;
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_FOUND_IN_CACHE_FROM_LOCATION_IT_WAS_SUCCESSFUL:
        u32 = 6393;
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_FOUND_IN_CACHE_FROM_LOCATION_IT_WAS_SUCCESSFUL_2:
        u32 = 6394;
    pub const REUSING_RESOLUTION_OF_MODULE_FROM_FOUND_IN_CACHE_FROM_LOCATION_IT_WAS_NOT_RESOLV:
        u32 = 6395;
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_FOUND_IN_CACHE_FROM_LOCATION:
        u32 = 6396;
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_FOUND_IN_CACHE_FROM_LOCATION_2:
        u32 = 6397;
    pub const REUSING_RESOLUTION_OF_TYPE_REFERENCE_DIRECTIVE_FROM_FOUND_IN_CACHE_FROM_LOCATION_3:
        u32 = 6398;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THAT_SOME_OF_THE_CHANGES:
        u32 = 6399;
    pub const PROJECT_IS_UP_TO_DATE_BUT_NEEDS_TO_UPDATE_TIMESTAMPS_OF_OUTPUT_FILES_THAT_ARE_OL:
        u32 = 6400;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_THERE_WAS_ERROR_READING_FILE: u32 = 6401;
    pub const RESOLVING_IN_MODE_WITH_CONDITIONS: u32 = 6402;
    pub const MATCHED_CONDITION: u32 = 6403;
    pub const USING_SUBPATH_WITH_TARGET: u32 = 6404;
    pub const SAW_NON_MATCHING_CONDITION: u32 = 6405;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THERE_IS_CHANGE_IN_COMPI:
        u32 = 6406;
    pub const ALLOW_IMPORTS_TO_INCLUDE_TYPESCRIPT_FILE_EXTENSIONS_REQUIRES_MODULERESOLUTION_BU:
        u32 = 6407;
    pub const USE_THE_PACKAGE_JSON_EXPORTS_FIELD_WHEN_RESOLVING_PACKAGE_IMPORTS: u32 = 6408;
    pub const USE_THE_PACKAGE_JSON_IMPORTS_FIELD_WHEN_RESOLVING_IMPORTS: u32 = 6409;
    pub const CONDITIONS_TO_SET_IN_ADDITION_TO_THE_RESOLVER_SPECIFIC_DEFAULTS_WHEN_RESOLVING_I:
        u32 = 6410;
    pub const TRUE_WHEN_MODULERESOLUTION_IS_NODE16_NODENEXT_OR_BUNDLER_OTHERWISE_FALSE: u32 = 6411;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THAT_FILE_WAS_ROOT_FILE: u32 =
        6412;
    pub const ENTERING_CONDITIONAL_EXPORTS: u32 = 6413;
    pub const RESOLVED_UNDER_CONDITION: u32 = 6414;
    pub const FAILED_TO_RESOLVE_UNDER_CONDITION: u32 = 6415;
    pub const EXITING_CONDITIONAL_EXPORTS: u32 = 6416;
    pub const SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_PREFERRED_EXTENSIONS: u32 = 6417;
    pub const SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_FALLBACK_EXTENSIONS: u32 = 6418;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE_BUILDINFO_FILE_INDICATES_THAT_PROGRAM_NEEDS_TO_RE:
        u32 = 6419;
    pub const PROJECT_IS_OUT_OF_DATE_BECAUSE: u32 = 6420;
    pub const REWRITE_TS_TSX_MTS_AND_CTS_FILE_EXTENSIONS_IN_RELATIVE_IMPORT_PATHS_TO_THEIR_JAV:
        u32 = 6421;
    pub const THE_EXPECTED_TYPE_COMES_FROM_PROPERTY_WHICH_IS_DECLARED_HERE_ON_TYPE: u32 = 6500;
    pub const THE_EXPECTED_TYPE_COMES_FROM_THIS_INDEX_SIGNATURE: u32 = 6501;
    pub const THE_EXPECTED_TYPE_COMES_FROM_THE_RETURN_TYPE_OF_THIS_SIGNATURE: u32 = 6502;
    pub const PRINT_NAMES_OF_FILES_THAT_ARE_PART_OF_THE_COMPILATION_AND_THEN_STOP_PROCESSING: u32 =
        6503;
    pub const FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION: u32 = 6504;
    pub const PRINT_NAMES_OF_FILES_AND_THE_REASON_THEY_ARE_PART_OF_THE_COMPILATION: u32 = 6505;
    pub const CONSIDER_ADDING_A_DECLARE_MODIFIER_TO_THIS_CLASS: u32 = 6506;
    pub const ALLOW_JAVASCRIPT_FILES_TO_BE_A_PART_OF_YOUR_PROGRAM_USE_THE_CHECKJS_OPTION_TO_GE:
        u32 = 6600;
    pub const ALLOW_IMPORT_X_FROM_Y_WHEN_A_MODULE_DOESNT_HAVE_A_DEFAULT_EXPORT: u32 = 6601;
    pub const ALLOW_ACCESSING_UMD_GLOBALS_FROM_MODULES: u32 = 6602;
    pub const DISABLE_ERROR_REPORTING_FOR_UNREACHABLE_CODE: u32 = 6603;
    pub const DISABLE_ERROR_REPORTING_FOR_UNUSED_LABELS: u32 = 6604;
    pub const ENSURE_USE_STRICT_IS_ALWAYS_EMITTED: u32 = 6605;
    pub const HAVE_RECOMPILES_IN_PROJECTS_THAT_USE_INCREMENTAL_AND_WATCH_MODE_ASSUME_THAT_CHAN:
        u32 = 6606;
    pub const SPECIFY_THE_BASE_DIRECTORY_TO_RESOLVE_NON_RELATIVE_MODULE_NAMES: u32 = 6607;
    pub const NO_LONGER_SUPPORTED_IN_EARLY_VERSIONS_MANUALLY_SET_THE_TEXT_ENCODING_FOR_READING:
        u32 = 6608;
    pub const ENABLE_ERROR_REPORTING_IN_TYPE_CHECKED_JAVASCRIPT_FILES: u32 = 6609;
    pub const ENABLE_CONSTRAINTS_THAT_ALLOW_A_TYPESCRIPT_PROJECT_TO_BE_USED_WITH_PROJECT_REFER:
        u32 = 6611;
    pub const GENERATE_D_TS_FILES_FROM_TYPESCRIPT_AND_JAVASCRIPT_FILES_IN_YOUR_PROJECT: u32 = 6612;
    pub const SPECIFY_THE_OUTPUT_DIRECTORY_FOR_GENERATED_DECLARATION_FILES: u32 = 6613;
    pub const CREATE_SOURCEMAPS_FOR_D_TS_FILES: u32 = 6614;
    pub const OUTPUT_COMPILER_PERFORMANCE_INFORMATION_AFTER_BUILDING: u32 = 6615;
    pub const DISABLES_INFERENCE_FOR_TYPE_ACQUISITION_BY_LOOKING_AT_FILENAMES_IN_A_PROJECT: u32 =
        6616;
    pub const REDUCE_THE_NUMBER_OF_PROJECTS_LOADED_AUTOMATICALLY_BY_TYPESCRIPT: u32 = 6617;
    pub const REMOVE_THE_20MB_CAP_ON_TOTAL_SOURCE_CODE_SIZE_FOR_JAVASCRIPT_FILES_IN_THE_TYPESC:
        u32 = 6618;
    pub const OPT_A_PROJECT_OUT_OF_MULTI_PROJECT_REFERENCE_CHECKING_WHEN_EDITING: u32 = 6619;
    pub const DISABLE_PREFERRING_SOURCE_FILES_INSTEAD_OF_DECLARATION_FILES_WHEN_REFERENCING_CO:
        u32 = 6620;
    pub const EMIT_MORE_COMPLIANT_BUT_VERBOSE_AND_LESS_PERFORMANT_JAVASCRIPT_FOR_ITERATION: u32 =
        6621;
    pub const EMIT_A_UTF_8_BYTE_ORDER_MARK_BOM_IN_THE_BEGINNING_OF_OUTPUT_FILES: u32 = 6622;
    pub const ONLY_OUTPUT_D_TS_FILES_AND_NOT_JAVASCRIPT_FILES: u32 = 6623;
    pub const EMIT_DESIGN_TYPE_METADATA_FOR_DECORATED_DECLARATIONS_IN_SOURCE_FILES: u32 = 6624;
    pub const DISABLE_THE_TYPE_ACQUISITION_FOR_JAVASCRIPT_PROJECTS: u32 = 6625;
    pub const EMIT_ADDITIONAL_JAVASCRIPT_TO_EASE_SUPPORT_FOR_IMPORTING_COMMONJS_MODULES_THIS_E:
        u32 = 6626;
    pub const FILTERS_RESULTS_FROM_THE_INCLUDE_OPTION: u32 = 6627;
    pub const REMOVE_A_LIST_OF_DIRECTORIES_FROM_THE_WATCH_PROCESS: u32 = 6628;
    pub const REMOVE_A_LIST_OF_FILES_FROM_THE_WATCH_MODES_PROCESSING: u32 = 6629;
    pub const ENABLE_EXPERIMENTAL_SUPPORT_FOR_LEGACY_EXPERIMENTAL_DECORATORS: u32 = 6630;
    pub const PRINT_FILES_READ_DURING_THE_COMPILATION_INCLUDING_WHY_IT_WAS_INCLUDED: u32 = 6631;
    pub const OUTPUT_MORE_DETAILED_COMPILER_PERFORMANCE_INFORMATION_AFTER_BUILDING: u32 = 6632;
    pub const SPECIFY_ONE_OR_MORE_PATH_OR_NODE_MODULE_REFERENCES_TO_BASE_CONFIGURATION_FILES_F:
        u32 = 6633;
    pub const SPECIFY_WHAT_APPROACH_THE_WATCHER_SHOULD_USE_IF_THE_SYSTEM_RUNS_OUT_OF_NATIVE_FI:
        u32 = 6634;
    pub const INCLUDE_A_LIST_OF_FILES_THIS_DOES_NOT_SUPPORT_GLOB_PATTERNS_AS_OPPOSED_TO_INCLUD:
        u32 = 6635;
    pub const BUILD_ALL_PROJECTS_INCLUDING_THOSE_THAT_APPEAR_TO_BE_UP_TO_DATE: u32 = 6636;
    pub const ENSURE_THAT_CASING_IS_CORRECT_IN_IMPORTS: u32 = 6637;
    pub const EMIT_A_V8_CPU_PROFILE_OF_THE_COMPILER_RUN_FOR_DEBUGGING: u32 = 6638;
    pub const ALLOW_IMPORTING_HELPER_FUNCTIONS_FROM_TSLIB_ONCE_PER_PROJECT_INSTEAD_OF_INCLUDIN:
        u32 = 6639;
    pub const SKIP_BUILDING_DOWNSTREAM_PROJECTS_ON_ERROR_IN_UPSTREAM_PROJECT: u32 = 6640;
    pub const SPECIFY_A_LIST_OF_GLOB_PATTERNS_THAT_MATCH_FILES_TO_BE_INCLUDED_IN_COMPILATION: u32 =
        6641;
    pub const SAVE_TSBUILDINFO_FILES_TO_ALLOW_FOR_INCREMENTAL_COMPILATION_OF_PROJECTS: u32 = 6642;
    pub const INCLUDE_SOURCEMAP_FILES_INSIDE_THE_EMITTED_JAVASCRIPT: u32 = 6643;
    pub const INCLUDE_SOURCE_CODE_IN_THE_SOURCEMAPS_INSIDE_THE_EMITTED_JAVASCRIPT: u32 = 6644;
    pub const ENSURE_THAT_EACH_FILE_CAN_BE_SAFELY_TRANSPILED_WITHOUT_RELYING_ON_OTHER_IMPORTS: u32 =
        6645;
    pub const SPECIFY_WHAT_JSX_CODE_IS_GENERATED: u32 = 6646;
    pub const SPECIFY_THE_JSX_FACTORY_FUNCTION_USED_WHEN_TARGETING_REACT_JSX_EMIT_E_G_REACT_CR:
        u32 = 6647;
    pub const SPECIFY_THE_JSX_FRAGMENT_REFERENCE_USED_FOR_FRAGMENTS_WHEN_TARGETING_REACT_JSX_E:
        u32 = 6648;
    pub const SPECIFY_MODULE_SPECIFIER_USED_TO_IMPORT_THE_JSX_FACTORY_FUNCTIONS_WHEN_USING_JSX:
        u32 = 6649;
    pub const MAKE_KEYOF_ONLY_RETURN_STRINGS_INSTEAD_OF_STRING_NUMBERS_OR_SYMBOLS_LEGACY_OPTIO:
        u32 = 6650;
    pub const SPECIFY_A_SET_OF_BUNDLED_LIBRARY_DECLARATION_FILES_THAT_DESCRIBE_THE_TARGET_RUNT:
        u32 = 6651;
    pub const PRINT_THE_NAMES_OF_EMITTED_FILES_AFTER_A_COMPILATION: u32 = 6652;
    pub const PRINT_ALL_OF_THE_FILES_READ_DURING_THE_COMPILATION: u32 = 6653;
    pub const SET_THE_LANGUAGE_OF_THE_MESSAGING_FROM_TYPESCRIPT_THIS_DOES_NOT_AFFECT_EMIT: u32 =
        6654;
    pub const SPECIFY_THE_LOCATION_WHERE_DEBUGGER_SHOULD_LOCATE_MAP_FILES_INSTEAD_OF_GENERATED:
        u32 = 6655;
    pub const SPECIFY_THE_MAXIMUM_FOLDER_DEPTH_USED_FOR_CHECKING_JAVASCRIPT_FILES_FROM_NODE_MO:
        u32 = 6656;
    pub const SPECIFY_WHAT_MODULE_CODE_IS_GENERATED: u32 = 6657;
    pub const SPECIFY_HOW_TYPESCRIPT_LOOKS_UP_A_FILE_FROM_A_GIVEN_MODULE_SPECIFIER: u32 = 6658;
    pub const SET_THE_NEWLINE_CHARACTER_FOR_EMITTING_FILES: u32 = 6659;
    pub const DISABLE_EMITTING_FILES_FROM_A_COMPILATION: u32 = 6660;
    pub const DISABLE_GENERATING_CUSTOM_HELPER_FUNCTIONS_LIKE_EXTENDS_IN_COMPILED_OUTPUT: u32 =
        6661;
    pub const DISABLE_EMITTING_FILES_IF_ANY_TYPE_CHECKING_ERRORS_ARE_REPORTED: u32 = 6662;
    pub const DISABLE_TRUNCATING_TYPES_IN_ERROR_MESSAGES: u32 = 6663;
    pub const ENABLE_ERROR_REPORTING_FOR_FALLTHROUGH_CASES_IN_SWITCH_STATEMENTS: u32 = 6664;
    pub const ENABLE_ERROR_REPORTING_FOR_EXPRESSIONS_AND_DECLARATIONS_WITH_AN_IMPLIED_ANY_TYPE:
        u32 = 6665;
    pub const ENSURE_OVERRIDING_MEMBERS_IN_DERIVED_CLASSES_ARE_MARKED_WITH_AN_OVERRIDE_MODIFIE:
        u32 = 6666;
    pub const ENABLE_ERROR_REPORTING_FOR_CODEPATHS_THAT_DO_NOT_EXPLICITLY_RETURN_IN_A_FUNCTION:
        u32 = 6667;
    pub const ENABLE_ERROR_REPORTING_WHEN_THIS_IS_GIVEN_THE_TYPE_ANY: u32 = 6668;
    pub const DISABLE_ADDING_USE_STRICT_DIRECTIVES_IN_EMITTED_JAVASCRIPT_FILES: u32 = 6669;
    pub const DISABLE_INCLUDING_ANY_LIBRARY_FILES_INCLUDING_THE_DEFAULT_LIB_D_TS: u32 = 6670;
    pub const ENFORCES_USING_INDEXED_ACCESSORS_FOR_KEYS_DECLARED_USING_AN_INDEXED_TYPE: u32 = 6671;
    pub const DISALLOW_IMPORTS_REQUIRES_OR_REFERENCE_S_FROM_EXPANDING_THE_NUMBER_OF_FILES_TYPE:
        u32 = 6672;
    pub const DISABLE_STRICT_CHECKING_OF_GENERIC_SIGNATURES_IN_FUNCTION_TYPES: u32 = 6673;
    pub const ADD_UNDEFINED_TO_A_TYPE_WHEN_ACCESSED_USING_AN_INDEX: u32 = 6674;
    pub const ENABLE_ERROR_REPORTING_WHEN_LOCAL_VARIABLES_ARENT_READ: u32 = 6675;
    pub const RAISE_AN_ERROR_WHEN_A_FUNCTION_PARAMETER_ISNT_READ: u32 = 6676;
    pub const DEPRECATED_SETTING_USE_OUTFILE_INSTEAD: u32 = 6677;
    pub const SPECIFY_AN_OUTPUT_FOLDER_FOR_ALL_EMITTED_FILES: u32 = 6678;
    pub const SPECIFY_A_FILE_THAT_BUNDLES_ALL_OUTPUTS_INTO_ONE_JAVASCRIPT_FILE_IF_DECLARATION: u32 =
        6679;
    pub const SPECIFY_A_SET_OF_ENTRIES_THAT_RE_MAP_IMPORTS_TO_ADDITIONAL_LOOKUP_LOCATIONS: u32 =
        6680;
    pub const SPECIFY_A_LIST_OF_LANGUAGE_SERVICE_PLUGINS_TO_INCLUDE: u32 = 6681;
    pub const DISABLE_ERASING_CONST_ENUM_DECLARATIONS_IN_GENERATED_CODE: u32 = 6682;
    pub const DISABLE_RESOLVING_SYMLINKS_TO_THEIR_REALPATH_THIS_CORRELATES_TO_THE_SAME_FLAG_IN:
        u32 = 6683;
    pub const DISABLE_WIPING_THE_CONSOLE_IN_WATCH_MODE: u32 = 6684;
    pub const ENABLE_COLOR_AND_FORMATTING_IN_TYPESCRIPTS_OUTPUT_TO_MAKE_COMPILER_ERRORS_EASIER:
        u32 = 6685;
    pub const SPECIFY_THE_OBJECT_INVOKED_FOR_CREATEELEMENT_THIS_ONLY_APPLIES_WHEN_TARGETING_RE:
        u32 = 6686;
    pub const SPECIFY_AN_ARRAY_OF_OBJECTS_THAT_SPECIFY_PATHS_FOR_PROJECTS_USED_IN_PROJECT_REFE:
        u32 = 6687;
    pub const DISABLE_EMITTING_COMMENTS: u32 = 6688;
    pub const ENABLE_IMPORTING_JSON_FILES: u32 = 6689;
    pub const SPECIFY_THE_ROOT_FOLDER_WITHIN_YOUR_SOURCE_FILES: u32 = 6690;
    pub const ALLOW_MULTIPLE_FOLDERS_TO_BE_TREATED_AS_ONE_WHEN_RESOLVING_MODULES: u32 = 6691;
    pub const SKIP_TYPE_CHECKING_D_TS_FILES_THAT_ARE_INCLUDED_WITH_TYPESCRIPT: u32 = 6692;
    pub const SKIP_TYPE_CHECKING_ALL_D_TS_FILES: u32 = 6693;
    pub const CREATE_SOURCE_MAP_FILES_FOR_EMITTED_JAVASCRIPT_FILES: u32 = 6694;
    pub const SPECIFY_THE_ROOT_PATH_FOR_DEBUGGERS_TO_FIND_THE_REFERENCE_SOURCE_CODE: u32 = 6695;
    pub const CHECK_THAT_THE_ARGUMENTS_FOR_BIND_CALL_AND_APPLY_METHODS_MATCH_THE_ORIGINAL_FUNC:
        u32 = 6697;
    pub const WHEN_ASSIGNING_FUNCTIONS_CHECK_TO_ENSURE_PARAMETERS_AND_THE_RETURN_VALUES_ARE_SU:
        u32 = 6698;
    pub const WHEN_TYPE_CHECKING_TAKE_INTO_ACCOUNT_NULL_AND_UNDEFINED: u32 = 6699;
    pub const CHECK_FOR_CLASS_PROPERTIES_THAT_ARE_DECLARED_BUT_NOT_SET_IN_THE_CONSTRUCTOR: u32 =
        6700;
    pub const DISABLE_EMITTING_DECLARATIONS_THAT_HAVE_INTERNAL_IN_THEIR_JSDOC_COMMENTS: u32 = 6701;
    pub const DISABLE_REPORTING_OF_EXCESS_PROPERTY_ERRORS_DURING_THE_CREATION_OF_OBJECT_LITERA:
        u32 = 6702;
    pub const SUPPRESS_NOIMPLICITANY_ERRORS_WHEN_INDEXING_OBJECTS_THAT_LACK_INDEX_SIGNATURES: u32 =
        6703;
    pub const SYNCHRONOUSLY_CALL_CALLBACKS_AND_UPDATE_THE_STATE_OF_DIRECTORY_WATCHERS_ON_PLATF:
        u32 = 6704;
    pub const SET_THE_JAVASCRIPT_LANGUAGE_VERSION_FOR_EMITTED_JAVASCRIPT_AND_INCLUDE_COMPATIBL:
        u32 = 6705;
    pub const LOG_PATHS_USED_DURING_THE_MODULERESOLUTION_PROCESS: u32 = 6706;
    pub const SPECIFY_THE_PATH_TO_TSBUILDINFO_INCREMENTAL_COMPILATION_FILE: u32 = 6707;
    pub const SPECIFY_OPTIONS_FOR_AUTOMATIC_ACQUISITION_OF_DECLARATION_FILES: u32 = 6709;
    pub const SPECIFY_MULTIPLE_FOLDERS_THAT_ACT_LIKE_NODE_MODULES_TYPES: u32 = 6710;
    pub const SPECIFY_TYPE_PACKAGE_NAMES_TO_BE_INCLUDED_WITHOUT_BEING_REFERENCED_IN_A_SOURCE_F:
        u32 = 6711;
    pub const EMIT_ECMASCRIPT_STANDARD_COMPLIANT_CLASS_FIELDS: u32 = 6712;
    pub const ENABLE_VERBOSE_LOGGING: u32 = 6713;
    pub const SPECIFY_HOW_DIRECTORIES_ARE_WATCHED_ON_SYSTEMS_THAT_LACK_RECURSIVE_FILE_WATCHING:
        u32 = 6714;
    pub const SPECIFY_HOW_THE_TYPESCRIPT_WATCH_MODE_WORKS: u32 = 6715;
    pub const REQUIRE_UNDECLARED_PROPERTIES_FROM_INDEX_SIGNATURES_TO_USE_ELEMENT_ACCESSES: u32 =
        6717;
    pub const SPECIFY_EMIT_CHECKING_BEHAVIOR_FOR_IMPORTS_THAT_ARE_ONLY_USED_FOR_TYPES: u32 = 6718;
    pub const REQUIRE_SUFFICIENT_ANNOTATION_ON_EXPORTS_SO_OTHER_TOOLS_CAN_TRIVIALLY_GENERATE_D:
        u32 = 6719;
    pub const BUILT_IN_ITERATORS_ARE_INSTANTIATED_WITH_A_TRETURN_TYPE_OF_UNDEFINED_INSTEAD_OF: u32 =
        6720;
    pub const DO_NOT_ALLOW_RUNTIME_CONSTRUCTS_THAT_ARE_NOT_PART_OF_ECMASCRIPT: u32 = 6721;
    pub const DEFAULT_CATCH_CLAUSE_VARIABLES_AS_UNKNOWN_INSTEAD_OF_ANY: u32 = 6803;
    pub const DO_NOT_TRANSFORM_OR_ELIDE_ANY_IMPORTS_OR_EXPORTS_NOT_MARKED_AS_TYPE_ONLY_ENSURIN:
        u32 = 6804;
    pub const DISABLE_FULL_TYPE_CHECKING_ONLY_CRITICAL_PARSE_AND_EMIT_ERRORS_WILL_BE_REPORTED: u32 =
        6805;
    pub const CHECK_SIDE_EFFECT_IMPORTS: u32 = 6806;
    pub const THIS_OPERATION_CAN_BE_SIMPLIFIED_THIS_SHIFT_IS_IDENTICAL_TO: u32 = 6807;
    pub const ENABLE_LIB_REPLACEMENT: u32 = 6808;
    pub const ENSURE_TYPES_ARE_ORDERED_STABLY_AND_DETERMINISTICALLY_ACROSS_COMPILATIONS: u32 = 6809;
    pub const ONE_OF: u32 = 6900;
    pub const ONE_OR_MORE: u32 = 6901;
    pub const TYPE: u32 = 6902;
    pub const DEFAULT: u32 = 6903;
    pub const TRUE_UNLESS_STRICT_IS_FALSE: u32 = 6905;
    pub const FALSE_UNLESS_COMPOSITE_IS_SET: u32 = 6906;
    pub const NODE_MODULES_BOWER_COMPONENTS_JSPM_PACKAGES_PLUS_THE_VALUE_OF_OUTDIR_IF_ONE_IS_S:
        u32 = 6907;
    pub const IF_FILES_IS_SPECIFIED_OTHERWISE: u32 = 6908;
    pub const TRUE_IF_COMPOSITE_FALSE_OTHERWISE: u32 = 6909;
    pub const COMPUTED_FROM_THE_LIST_OF_INPUT_FILES: u32 = 6911;
    pub const PLATFORM_SPECIFIC: u32 = 6912;
    pub const YOU_CAN_LEARN_ABOUT_ALL_OF_THE_COMPILER_OPTIONS_AT: u32 = 6913;
    pub const INCLUDING_WATCH_W_WILL_START_WATCHING_THE_CURRENT_PROJECT_FOR_THE_FILE_CHANGES_O:
        u32 = 6914;
    pub const USING_BUILD_B_WILL_MAKE_TSC_BEHAVE_MORE_LIKE_A_BUILD_ORCHESTRATOR_THAN_A_COMPILE:
        u32 = 6915;
    pub const COMMON_COMMANDS: u32 = 6916;
    pub const ALL_COMPILER_OPTIONS: u32 = 6917;
    pub const WATCH_OPTIONS: u32 = 6918;
    pub const BUILD_OPTIONS: u32 = 6919;
    pub const COMMON_COMPILER_OPTIONS: u32 = 6920;
    pub const COMMAND_LINE_FLAGS: u32 = 6921;
    pub const TSC_THE_TYPESCRIPT_COMPILER: u32 = 6922;
    pub const COMPILES_THE_CURRENT_PROJECT_TSCONFIG_JSON_IN_THE_WORKING_DIRECTORY: u32 = 6923;
    pub const IGNORING_TSCONFIG_JSON_COMPILES_THE_SPECIFIED_FILES_WITH_DEFAULT_COMPILER_OPTION:
        u32 = 6924;
    pub const BUILD_A_COMPOSITE_PROJECT_IN_THE_WORKING_DIRECTORY: u32 = 6925;
    pub const CREATES_A_TSCONFIG_JSON_WITH_THE_RECOMMENDED_SETTINGS_IN_THE_WORKING_DIRECTORY: u32 =
        6926;
    pub const COMPILES_THE_TYPESCRIPT_PROJECT_LOCATED_AT_THE_SPECIFIED_PATH: u32 = 6927;
    pub const AN_EXPANDED_VERSION_OF_THIS_INFORMATION_SHOWING_ALL_POSSIBLE_COMPILER_OPTIONS: u32 =
        6928;
    pub const COMPILES_THE_CURRENT_PROJECT_WITH_ADDITIONAL_SETTINGS: u32 = 6929;
    pub const TRUE_FOR_ES2022_AND_ABOVE_INCLUDING_ESNEXT: u32 = 6930;
    pub const LIST_OF_FILE_NAME_SUFFIXES_TO_SEARCH_WHEN_RESOLVING_A_MODULE: u32 = 6931;
    pub const FALSE_UNLESS_CHECKJS_IS_SET: u32 = 6932;
    pub const VARIABLE_IMPLICITLY_HAS_AN_TYPE: u32 = 7005;
    pub const PARAMETER_IMPLICITLY_HAS_AN_TYPE: u32 = 7006;
    pub const MEMBER_IMPLICITLY_HAS_AN_TYPE: u32 = 7008;
    pub const NEW_EXPRESSION_WHOSE_TARGET_LACKS_A_CONSTRUCT_SIGNATURE_IMPLICITLY_HAS_AN_ANY_TY:
        u32 = 7009;
    pub const WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE: u32 = 7010;
    pub const FUNCTION_EXPRESSION_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN: u32 =
        7011;
    pub const THIS_OVERLOAD_IMPLICITLY_RETURNS_THE_TYPE_BECAUSE_IT_LACKS_A_RETURN_TYPE_ANNOTAT:
        u32 = 7012;
    pub const CONSTRUCT_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RET:
        u32 = 7013;
    pub const FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE: u32 =
        7014;
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE:
        u32 = 7015;
    pub const COULD_NOT_FIND_A_DECLARATION_FILE_FOR_MODULE_IMPLICITLY_HAS_AN_ANY_TYPE: u32 = 7016;
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE: u32 = 7017;
    pub const OBJECT_LITERALS_PROPERTY_IMPLICITLY_HAS_AN_TYPE: u32 = 7018;
    pub const REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE: u32 = 7019;
    pub const CALL_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RETURN_T:
        u32 = 7020;
    pub const IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE:
        u32 = 7022;
    pub const IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION:
        u32 = 7023;
    pub const FUNCTION_IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_A:
        u32 = 7024;
    pub const GENERATOR_IMPLICITLY_HAS_YIELD_TYPE_CONSIDER_SUPPLYING_A_RETURN_TYPE_ANNOTATION: u32 =
        7025;
    pub const JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS: u32 = 7026;
    pub const UNREACHABLE_CODE_DETECTED: u32 = 7027;
    pub const UNUSED_LABEL: u32 = 7028;
    pub const FALLTHROUGH_CASE_IN_SWITCH: u32 = 7029;
    pub const NOT_ALL_CODE_PATHS_RETURN_A_VALUE: u32 = 7030;
    pub const BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE: u32 = 7031;
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE:
        u32 = 7032;
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_GET_ACCESSOR_LACKS_A_RETURN_TYPE_AN:
        u32 = 7033;
    pub const VARIABLE_IMPLICITLY_HAS_TYPE_IN_SOME_LOCATIONS_WHERE_ITS_TYPE_CANNOT_BE_DETERMIN:
        u32 = 7034;
    pub const TRY_NPM_I_SAVE_DEV_TYPES_IF_IT_EXISTS_OR_ADD_A_NEW_DECLARATION_D_TS_FILE_CONTAIN:
        u32 = 7035;
    pub const DYNAMIC_IMPORTS_SPECIFIER_MUST_BE_OF_TYPE_STRING_BUT_HERE_HAS_TYPE: u32 = 7036;
    pub const ENABLES_EMIT_INTEROPERABILITY_BETWEEN_COMMONJS_AND_ES_MODULES_VIA_CREATION_OF_NA:
        u32 = 7037;
    pub const TYPE_ORIGINATES_AT_THIS_IMPORT_A_NAMESPACE_STYLE_IMPORT_CANNOT_BE_CALLED_OR_CONS:
        u32 = 7038;
    pub const MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE: u32 = 7039;
    pub const IF_THE_PACKAGE_ACTUALLY_EXPOSES_THIS_MODULE_CONSIDER_SENDING_A_PULL_REQUEST_TO_A:
        u32 = 7040;
    pub const THE_CONTAINING_ARROW_FUNCTION_CAPTURES_THE_GLOBAL_VALUE_OF_THIS: u32 = 7041;
    pub const MODULE_WAS_RESOLVED_TO_BUT_RESOLVEJSONMODULE_IS_NOT_USED: u32 = 7042;
    pub const VARIABLE_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: u32 =
        7043;
    pub const PARAMETER_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: u32 =
        7044;
    pub const MEMBER_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: u32 =
        7045;
    pub const VARIABLE_IMPLICITLY_HAS_TYPE_IN_SOME_LOCATIONS_BUT_A_BETTER_TYPE_MAY_BE_INFERRED:
        u32 = 7046;
    pub const REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM:
        u32 = 7047;
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BUT_A_BETTER_TYPE_FOR_ITS_GET_ACCESSOR_MAY_BE_I:
        u32 = 7048;
    pub const PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BUT_A_BETTER_TYPE_FOR_ITS_SET_ACCESSOR_MAY_BE_I:
        u32 = 7049;
    pub const IMPLICITLY_HAS_AN_RETURN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE: u32 =
        7050;
    pub const PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN: u32 = 7051;
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE_DID_YOU_M:
        u32 = 7052;
    pub const ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN:
        u32 = 7053;
    pub const NO_INDEX_SIGNATURE_WITH_A_PARAMETER_OF_TYPE_WAS_FOUND_ON_TYPE: u32 = 7054;
    pub const WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_YIELD_TYPE: u32 = 7055;
    pub const THE_INFERRED_TYPE_OF_THIS_NODE_EXCEEDS_THE_MAXIMUM_LENGTH_THE_COMPILER_WILL_SERI:
        u32 = 7056;
    pub const YIELD_EXPRESSION_IMPLICITLY_RESULTS_IN_AN_ANY_TYPE_BECAUSE_ITS_CONTAINING_GENERA:
        u32 = 7057;
    pub const IF_THE_PACKAGE_ACTUALLY_EXPOSES_THIS_MODULE_TRY_ADDING_A_NEW_DECLARATION_D_TS_FI:
        u32 = 7058;
    pub const THIS_SYNTAX_IS_RESERVED_IN_FILES_WITH_THE_MTS_OR_CTS_EXTENSION_USE_AN_AS_EXPRESS:
        u32 = 7059;
    pub const THIS_SYNTAX_IS_RESERVED_IN_FILES_WITH_THE_MTS_OR_CTS_EXTENSION_ADD_A_TRAILING_CO:
        u32 = 7060;
    pub const A_MAPPED_TYPE_MAY_NOT_DECLARE_PROPERTIES_OR_METHODS: u32 = 7061;
    pub const YOU_CANNOT_RENAME_THIS_ELEMENT: u32 = 8000;
    pub const YOU_CANNOT_RENAME_ELEMENTS_THAT_ARE_DEFINED_IN_THE_STANDARD_TYPESCRIPT_LIBRARY: u32 =
        8001;
    pub const IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8002;
    pub const EXPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8003;
    pub const TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8004;
    pub const IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8005;
    pub const DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8006;
    pub const TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8008;
    pub const THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8009;
    pub const TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8010;
    pub const TYPE_ARGUMENTS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8011;
    pub const PARAMETER_MODIFIERS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8012;
    pub const NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8013;
    pub const TYPE_ASSERTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8016;
    pub const SIGNATURE_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8017;
    pub const REPORT_ERRORS_IN_JS_FILES: u32 = 8019;
    pub const JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS: u32 = 8020;
    pub const JSDOC_TYPEDEF_TAG_SHOULD_EITHER_HAVE_A_TYPE_ANNOTATION_OR_BE_FOLLOWED_BY_PROPERT:
        u32 = 8021;
    pub const JSDOC_IS_NOT_ATTACHED_TO_A_CLASS: u32 = 8022;
    pub const JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE: u32 = 8023;
    pub const JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME: u32 = 8024;
    pub const CLASS_DECLARATIONS_CANNOT_HAVE_MORE_THAN_ONE_AUGMENTS_OR_EXTENDS_TAG: u32 = 8025;
    pub const EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG: u32 = 8026;
    pub const EXPECTED_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG_2: u32 = 8027;
    pub const JSDOC_MAY_ONLY_APPEAR_IN_THE_LAST_PARAMETER_OF_A_SIGNATURE: u32 = 8028;
    pub const JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME_IT_WOULD_MATCH:
        u32 = 8029;
    pub const THE_TYPE_OF_A_FUNCTION_DECLARATION_MUST_MATCH_THE_FUNCTIONS_SIGNATURE: u32 = 8030;
    pub const YOU_CANNOT_RENAME_A_MODULE_VIA_A_GLOBAL_IMPORT: u32 = 8031;
    pub const QUALIFIED_NAME_IS_NOT_ALLOWED_WITHOUT_A_LEADING_PARAM_OBJECT: u32 = 8032;
    pub const A_JSDOC_TYPEDEF_COMMENT_MAY_NOT_CONTAIN_MULTIPLE_TYPE_TAGS: u32 = 8033;
    pub const THE_TAG_WAS_FIRST_SPECIFIED_HERE: u32 = 8034;
    pub const YOU_CANNOT_RENAME_ELEMENTS_THAT_ARE_DEFINED_IN_A_NODE_MODULES_FOLDER: u32 = 8035;
    pub const YOU_CANNOT_RENAME_ELEMENTS_THAT_ARE_DEFINED_IN_ANOTHER_NODE_MODULES_FOLDER: u32 =
        8036;
    pub const TYPE_SATISFACTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES: u32 = 8037;
    pub const DECORATORS_MAY_NOT_APPEAR_AFTER_EXPORT_OR_EXPORT_DEFAULT_IF_THEY_ALSO_APPEAR_BEF:
        u32 = 8038;
    pub const A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG: u32 = 8039;
    pub const DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_USING_PRIVATE_NAME_AN_EXPLICIT_TYPE_ANNO:
        u32 = 9005;
    pub const DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_USING_PRIVATE_NAME_FROM_MODULE_AN_EXPLIC:
        u32 = 9006;
    pub const FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: u32 =
        9007;
    pub const METHOD_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: u32 =
        9008;
    pub const AT_LEAST_ONE_ACCESSOR_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARA:
        u32 = 9009;
    pub const VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: u32 = 9010;
    pub const PARAMETER_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: u32 = 9011;
    pub const PROPERTY_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS: u32 = 9012;
    pub const EXPRESSION_TYPE_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS: u32 = 9013;
    pub const COMPUTED_PROPERTIES_MUST_BE_NUMBER_OR_STRING_LITERALS_VARIABLES_OR_DOTTED_EXPRES:
        u32 = 9014;
    pub const OBJECTS_THAT_CONTAIN_SPREAD_ASSIGNMENTS_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIO:
        u32 = 9015;
    pub const OBJECTS_THAT_CONTAIN_SHORTHAND_PROPERTIES_CANT_BE_INFERRED_WITH_ISOLATEDDECLARAT:
        u32 = 9016;
    pub const ONLY_CONST_ARRAYS_CAN_BE_INFERRED_WITH_ISOLATEDDECLARATIONS: u32 = 9017;
    pub const ARRAYS_WITH_SPREAD_ELEMENTS_CANT_INFERRED_WITH_ISOLATEDDECLARATIONS: u32 = 9018;
    pub const BINDING_ELEMENTS_CANT_BE_EXPORTED_DIRECTLY_WITH_ISOLATEDDECLARATIONS: u32 = 9019;
    pub const ENUM_MEMBER_INITIALIZERS_MUST_BE_COMPUTABLE_WITHOUT_REFERENCES_TO_EXTERNAL_SYMBO:
        u32 = 9020;
    pub const EXTENDS_CLAUSE_CANT_CONTAIN_AN_EXPRESSION_WITH_ISOLATEDDECLARATIONS: u32 = 9021;
    pub const INFERENCE_FROM_CLASS_EXPRESSIONS_IS_NOT_SUPPORTED_WITH_ISOLATEDDECLARATIONS: u32 =
        9022;
    pub const ASSIGNING_PROPERTIES_TO_FUNCTIONS_WITHOUT_DECLARING_THEM_IS_NOT_SUPPORTED_WITH_I:
        u32 = 9023;
    pub const DECLARATION_EMIT_FOR_THIS_PARAMETER_REQUIRES_IMPLICITLY_ADDING_UNDEFINED_TO_ITS: u32 =
        9025;
    pub const DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_PRESERVING_THIS_IMPORT_FOR_AUGMENTATIONS:
        u32 = 9026;
    pub const ADD_A_TYPE_ANNOTATION_TO_THE_VARIABLE: u32 = 9027;
    pub const ADD_A_TYPE_ANNOTATION_TO_THE_PARAMETER: u32 = 9028;
    pub const ADD_A_TYPE_ANNOTATION_TO_THE_PROPERTY: u32 = 9029;
    pub const ADD_A_RETURN_TYPE_TO_THE_FUNCTION_EXPRESSION: u32 = 9030;
    pub const ADD_A_RETURN_TYPE_TO_THE_FUNCTION_DECLARATION: u32 = 9031;
    pub const ADD_A_RETURN_TYPE_TO_THE_GET_ACCESSOR_DECLARATION: u32 = 9032;
    pub const ADD_A_TYPE_TO_PARAMETER_OF_THE_SET_ACCESSOR_DECLARATION: u32 = 9033;
    pub const ADD_A_RETURN_TYPE_TO_THE_METHOD: u32 = 9034;
    pub const ADD_SATISFIES_AND_A_TYPE_ASSERTION_TO_THIS_EXPRESSION_SATISFIES_T_AS_T_TO_MAKE_T:
        u32 = 9035;
    pub const MOVE_THE_EXPRESSION_IN_DEFAULT_EXPORT_TO_A_VARIABLE_AND_ADD_A_TYPE_ANNOTATION_TO:
        u32 = 9036;
    pub const DEFAULT_EXPORTS_CANT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS: u32 = 9037;
    pub const COMPUTED_PROPERTY_NAMES_ON_CLASS_OR_OBJECT_LITERALS_CANNOT_BE_INFERRED_WITH_ISOL:
        u32 = 9038;
    pub const TYPE_CONTAINING_PRIVATE_NAME_CANT_BE_USED_WITH_ISOLATEDDECLARATIONS: u32 = 9039;
    pub const JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION: u32 = 17000;
    pub const JSX_ELEMENTS_CANNOT_HAVE_MULTIPLE_ATTRIBUTES_WITH_THE_SAME_NAME: u32 = 17001;
    pub const EXPECTED_CORRESPONDING_JSX_CLOSING_TAG_FOR: u32 = 17002;
    pub const CANNOT_USE_JSX_UNLESS_THE_JSX_FLAG_IS_PROVIDED: u32 = 17004;
    pub const A_CONSTRUCTOR_CANNOT_CONTAIN_A_SUPER_CALL_WHEN_ITS_CLASS_EXTENDS_NULL: u32 = 17005;
    pub const AN_UNARY_EXPRESSION_WITH_THE_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN:
        u32 = 17006;
    pub const A_TYPE_ASSERTION_EXPRESSION_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN_EXPONENTI:
        u32 = 17007;
    pub const JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG: u32 = 17008;
    pub const SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS:
        u32 = 17009;
    pub const UNKNOWN_TYPE_ACQUISITION_OPTION: u32 = 17010;
    pub const SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF: u32 =
        17011;
    pub const IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN: u32 = 17012;
    pub const META_PROPERTY_IS_ONLY_ALLOWED_IN_THE_BODY_OF_A_FUNCTION_DECLARATION_FUNCTION_EXP:
        u32 = 17013;
    pub const JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG: u32 = 17014;
    pub const EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT: u32 = 17015;
    pub const THE_JSXFRAGMENTFACTORY_COMPILER_OPTION_MUST_BE_PROVIDED_TO_USE_JSX_FRAGMENTS_WIT:
        u32 = 17016;
    pub const AN_JSXFRAG_PRAGMA_IS_REQUIRED_WHEN_USING_AN_JSX_PRAGMA_WITH_JSX_FRAGMENTS: u32 =
        17017;
    pub const UNKNOWN_TYPE_ACQUISITION_OPTION_DID_YOU_MEAN: u32 = 17018;
    pub const AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE: u32 =
        17019;
    pub const AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE: u32 =
        17020;
    pub const UNICODE_ESCAPE_SEQUENCE_CANNOT_APPEAR_HERE: u32 = 17021;
    pub const CIRCULARITY_DETECTED_WHILE_RESOLVING_CONFIGURATION: u32 = 18000;
    pub const THE_FILES_LIST_IN_CONFIG_FILE_IS_EMPTY: u32 = 18002;
    pub const NO_INPUTS_WERE_FOUND_IN_CONFIG_FILE_SPECIFIED_INCLUDE_PATHS_WERE_AND_EXCLUDE_PAT:
        u32 = 18003;
    pub const NO_VALUE_EXISTS_IN_SCOPE_FOR_THE_SHORTHAND_PROPERTY_EITHER_DECLARE_ONE_OR_PROVID:
        u32 = 18004;
    pub const CLASSES_MAY_NOT_HAVE_A_FIELD_NAMED_CONSTRUCTOR: u32 = 18006;
    pub const JSX_EXPRESSIONS_MAY_NOT_USE_THE_COMMA_OPERATOR_DID_YOU_MEAN_TO_WRITE_AN_ARRAY: u32 =
        18007;
    pub const PRIVATE_IDENTIFIERS_CANNOT_BE_USED_AS_PARAMETERS: u32 = 18009;
    pub const AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER: u32 = 18010;
    pub const THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_PRIVATE_IDENTIFIER: u32 = 18011;
    pub const CONSTRUCTOR_IS_A_RESERVED_WORD: u32 = 18012;
    pub const PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER: u32 =
        18013;
    pub const THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED:
        u32 = 18014;
    pub const PROPERTY_IN_TYPE_REFERS_TO_A_DIFFERENT_MEMBER_THAT_CANNOT_BE_ACCESSED_FROM_WITHI:
        u32 = 18015;
    pub const PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES: u32 = 18016;
    pub const THE_SHADOWING_DECLARATION_OF_IS_DEFINED_HERE: u32 = 18017;
    pub const THE_DECLARATION_OF_THAT_YOU_PROBABLY_INTENDED_TO_USE_IS_DEFINED_HERE: u32 = 18018;
    pub const MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER: u32 = 18019;
    pub const AN_ENUM_MEMBER_CANNOT_BE_NAMED_WITH_A_PRIVATE_IDENTIFIER: u32 = 18024;
    pub const CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE: u32 = 18026;
    pub const COMPILER_RESERVES_NAME_WHEN_EMITTING_PRIVATE_IDENTIFIER_DOWNLEVEL: u32 = 18027;
    pub const PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER:
        u32 = 18028;
    pub const PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_IN_VARIABLE_DECLARATIONS: u32 = 18029;
    pub const AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS: u32 = 18030;
    pub const THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN: u32 =
        18031;
    pub const THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_EXISTS_IN_MULTIPLE_CONSTI:
        u32 = 18032;
    pub const TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_REQUIRED_FOR_COMPUTED_ENUM_MEMBER_VALUES: u32 =
        18033;
    pub const SPECIFY_THE_JSX_FRAGMENT_FACTORY_FUNCTION_TO_USE_WHEN_TARGETING_REACT_JSX_EMIT_W:
        u32 = 18034;
    pub const INVALID_VALUE_FOR_JSXFRAGMENTFACTORY_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME:
        u32 = 18035;
    pub const CLASS_DECORATORS_CANT_BE_USED_WITH_STATIC_PRIVATE_IDENTIFIER_CONSIDER_REMOVING_T:
        u32 = 18036;
    pub const AWAIT_EXPRESSION_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: u32 = 18037;
    pub const FOR_AWAIT_LOOPS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: u32 = 18038;
    pub const INVALID_USE_OF_IT_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: u32 = 18039;
    pub const A_RETURN_STATEMENT_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: u32 = 18041;
    pub const IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT:
        u32 = 18042;
    pub const TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES: u32 = 18043;
    pub const IS_AUTOMATICALLY_EXPORTED_HERE: u32 = 18044;
    pub const PROPERTIES_WITH_THE_ACCESSOR_MODIFIER_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRI:
        u32 = 18045;
    pub const IS_OF_TYPE_UNKNOWN: u32 = 18046;
    pub const IS_POSSIBLY_NULL: u32 = 18047;
    pub const IS_POSSIBLY_UNDEFINED: u32 = 18048;
    pub const IS_POSSIBLY_NULL_OR_UNDEFINED: u32 = 18049;
    pub const THE_VALUE_CANNOT_BE_USED_HERE: u32 = 18050;
    pub const COMPILER_OPTION_CANNOT_BE_GIVEN_AN_EMPTY_STRING: u32 = 18051;
    pub const ITS_TYPE_IS_NOT_A_VALID_JSX_ELEMENT_TYPE: u32 = 18053;
    pub const AWAIT_USING_STATEMENTS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK: u32 = 18054;
    pub const HAS_A_STRING_TYPE_BUT_MUST_HAVE_SYNTACTICALLY_RECOGNIZABLE_STRING_SYNTAX_WHEN_IS:
        u32 = 18055;
    pub const ENUM_MEMBER_FOLLOWING_A_NON_LITERAL_NUMERIC_MEMBER_MUST_HAVE_AN_INITIALIZER_WHEN:
        u32 = 18056;
    pub const STRING_LITERAL_IMPORT_AND_EXPORT_NAMES_ARE_NOT_SUPPORTED_WHEN_THE_MODULE_FLAG_IS:
        u32 = 18057;
    pub const DEFAULT_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT: u32 = 18058;
    pub const NAMED_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT: u32 = 18059;
    pub const DEFERRED_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ESNEXT_OR_PRE:
        u32 = 18060;
    pub const IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_IMPORT_DID_YOU_MEAN_META_OR_DEFER: u32 =
        18061;
    pub const NODENEXT_IF_MODULE_IS_NODENEXT_NODE16_IF_MODULE_IS_NODE16_OR_NODE18_OTHERWISE_BU:
        u32 = 69010;
    pub const FILE_IS_A_COMMONJS_MODULE_IT_MAY_BE_CONVERTED_TO_AN_ES_MODULE: u32 = 80001;
    pub const THIS_CONSTRUCTOR_FUNCTION_MAY_BE_CONVERTED_TO_A_CLASS_DECLARATION: u32 = 80002;
    pub const IMPORT_MAY_BE_CONVERTED_TO_A_DEFAULT_IMPORT: u32 = 80003;
    pub const JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES: u32 = 80004;
    pub const REQUIRE_CALL_MAY_BE_CONVERTED_TO_AN_IMPORT: u32 = 80005;
    pub const THIS_MAY_BE_CONVERTED_TO_AN_ASYNC_FUNCTION: u32 = 80006;
    pub const AWAIT_HAS_NO_EFFECT_ON_THE_TYPE_OF_THIS_EXPRESSION: u32 = 80007;
    pub const NUMERIC_LITERALS_WITH_ABSOLUTE_VALUES_EQUAL_TO_2_53_OR_GREATER_ARE_TOO_LARGE_TO: u32 =
        80008;
    pub const JSDOC_TYPEDEF_MAY_BE_CONVERTED_TO_TYPESCRIPT_TYPE: u32 = 80009;
    pub const JSDOC_TYPEDEFS_MAY_BE_CONVERTED_TO_TYPESCRIPT_TYPES: u32 = 80010;
    pub const ADD_MISSING_SUPER_CALL: u32 = 90001;
    pub const MAKE_SUPER_CALL_THE_FIRST_STATEMENT_IN_THE_CONSTRUCTOR: u32 = 90002;
    pub const CHANGE_EXTENDS_TO_IMPLEMENTS: u32 = 90003;
    pub const REMOVE_UNUSED_DECLARATION_FOR: u32 = 90004;
    pub const REMOVE_IMPORT_FROM: u32 = 90005;
    pub const IMPLEMENT_INTERFACE: u32 = 90006;
    pub const IMPLEMENT_INHERITED_ABSTRACT_CLASS: u32 = 90007;
    pub const ADD_TO_UNRESOLVED_VARIABLE: u32 = 90008;
    pub const REMOVE_VARIABLE_STATEMENT: u32 = 90010;
    pub const REMOVE_TEMPLATE_TAG: u32 = 90011;
    pub const REMOVE_TYPE_PARAMETERS: u32 = 90012;
    pub const IMPORT_FROM: u32 = 90013;
    pub const CHANGE_TO: u32 = 90014;
    pub const DECLARE_PROPERTY: u32 = 90016;
    pub const ADD_INDEX_SIGNATURE_FOR_PROPERTY: u32 = 90017;
    pub const DISABLE_CHECKING_FOR_THIS_FILE: u32 = 90018;
    pub const IGNORE_THIS_ERROR_MESSAGE: u32 = 90019;
    pub const INITIALIZE_PROPERTY_IN_THE_CONSTRUCTOR: u32 = 90020;
    pub const INITIALIZE_STATIC_PROPERTY: u32 = 90021;
    pub const CHANGE_SPELLING_TO: u32 = 90022;
    pub const DECLARE_METHOD: u32 = 90023;
    pub const DECLARE_STATIC_METHOD: u32 = 90024;
    pub const PREFIX_WITH_AN_UNDERSCORE: u32 = 90025;
    pub const REWRITE_AS_THE_INDEXED_ACCESS_TYPE: u32 = 90026;
    pub const DECLARE_STATIC_PROPERTY: u32 = 90027;
    pub const CALL_DECORATOR_EXPRESSION: u32 = 90028;
    pub const ADD_ASYNC_MODIFIER_TO_CONTAINING_FUNCTION: u32 = 90029;
    pub const REPLACE_INFER_WITH_UNKNOWN: u32 = 90030;
    pub const REPLACE_ALL_UNUSED_INFER_WITH_UNKNOWN: u32 = 90031;
    pub const ADD_PARAMETER_NAME: u32 = 90034;
    pub const DECLARE_PRIVATE_PROPERTY: u32 = 90035;
    pub const REPLACE_WITH_PROMISE: u32 = 90036;
    pub const FIX_ALL_INCORRECT_RETURN_TYPE_OF_AN_ASYNC_FUNCTIONS: u32 = 90037;
    pub const DECLARE_PRIVATE_METHOD: u32 = 90038;
    pub const REMOVE_UNUSED_DESTRUCTURING_DECLARATION: u32 = 90039;
    pub const REMOVE_UNUSED_DECLARATIONS_FOR: u32 = 90041;
    pub const DECLARE_A_PRIVATE_FIELD_NAMED: u32 = 90053;
    pub const INCLUDES_IMPORTS_OF_TYPES_REFERENCED_BY: u32 = 90054;
    pub const REMOVE_TYPE_FROM_IMPORT_DECLARATION_FROM: u32 = 90055;
    pub const REMOVE_TYPE_FROM_IMPORT_OF_FROM: u32 = 90056;
    pub const ADD_IMPORT_FROM: u32 = 90057;
    pub const UPDATE_IMPORT_FROM: u32 = 90058;
    pub const EXPORT_FROM_MODULE: u32 = 90059;
    pub const EXPORT_ALL_REFERENCED_LOCALS: u32 = 90060;
    pub const UPDATE_MODIFIERS_OF: u32 = 90061;
    pub const ADD_ANNOTATION_OF_TYPE: u32 = 90062;
    pub const ADD_RETURN_TYPE: u32 = 90063;
    pub const EXTRACT_BASE_CLASS_TO_VARIABLE: u32 = 90064;
    pub const EXTRACT_DEFAULT_EXPORT_TO_VARIABLE: u32 = 90065;
    pub const EXTRACT_BINDING_EXPRESSIONS_TO_VARIABLE: u32 = 90066;
    pub const ADD_ALL_MISSING_TYPE_ANNOTATIONS: u32 = 90067;
    pub const ADD_SATISFIES_AND_AN_INLINE_TYPE_ASSERTION_WITH: u32 = 90068;
    pub const EXTRACT_TO_VARIABLE_AND_REPLACE_WITH_AS_TYPEOF: u32 = 90069;
    pub const MARK_ARRAY_LITERAL_AS_CONST: u32 = 90070;
    pub const ANNOTATE_TYPES_OF_PROPERTIES_EXPANDO_FUNCTION_IN_A_NAMESPACE: u32 = 90071;
    pub const CONVERT_FUNCTION_TO_AN_ES2015_CLASS: u32 = 95001;
    pub const CONVERT_TO_IN: u32 = 95003;
    pub const EXTRACT_TO_IN: u32 = 95004;
    pub const EXTRACT_FUNCTION: u32 = 95005;
    pub const EXTRACT_CONSTANT: u32 = 95006;
    pub const EXTRACT_TO_IN_ENCLOSING_SCOPE: u32 = 95007;
    pub const EXTRACT_TO_IN_SCOPE: u32 = 95008;
    pub const ANNOTATE_WITH_TYPE_FROM_JSDOC: u32 = 95009;
    pub const INFER_TYPE_OF_FROM_USAGE: u32 = 95011;
    pub const INFER_PARAMETER_TYPES_FROM_USAGE: u32 = 95012;
    pub const CONVERT_TO_DEFAULT_IMPORT: u32 = 95013;
    pub const INSTALL: u32 = 95014;
    pub const REPLACE_IMPORT_WITH: u32 = 95015;
    pub const USE_SYNTHETIC_DEFAULT_MEMBER: u32 = 95016;
    pub const CONVERT_TO_ES_MODULE: u32 = 95017;
    pub const ADD_UNDEFINED_TYPE_TO_PROPERTY: u32 = 95018;
    pub const ADD_INITIALIZER_TO_PROPERTY: u32 = 95019;
    pub const ADD_DEFINITE_ASSIGNMENT_ASSERTION_TO_PROPERTY: u32 = 95020;
    pub const CONVERT_ALL_TYPE_LITERALS_TO_MAPPED_TYPE: u32 = 95021;
    pub const ADD_ALL_MISSING_MEMBERS: u32 = 95022;
    pub const INFER_ALL_TYPES_FROM_USAGE: u32 = 95023;
    pub const DELETE_ALL_UNUSED_DECLARATIONS: u32 = 95024;
    pub const PREFIX_ALL_UNUSED_DECLARATIONS_WITH_WHERE_POSSIBLE: u32 = 95025;
    pub const FIX_ALL_DETECTED_SPELLING_ERRORS: u32 = 95026;
    pub const ADD_INITIALIZERS_TO_ALL_UNINITIALIZED_PROPERTIES: u32 = 95027;
    pub const ADD_DEFINITE_ASSIGNMENT_ASSERTIONS_TO_ALL_UNINITIALIZED_PROPERTIES: u32 = 95028;
    pub const ADD_UNDEFINED_TYPE_TO_ALL_UNINITIALIZED_PROPERTIES: u32 = 95029;
    pub const CHANGE_ALL_JSDOC_STYLE_TYPES_TO_TYPESCRIPT: u32 = 95030;
    pub const CHANGE_ALL_JSDOC_STYLE_TYPES_TO_TYPESCRIPT_AND_ADD_UNDEFINED_TO_NULLABLE_TYPES: u32 =
        95031;
    pub const IMPLEMENT_ALL_UNIMPLEMENTED_INTERFACES: u32 = 95032;
    pub const INSTALL_ALL_MISSING_TYPES_PACKAGES: u32 = 95033;
    pub const REWRITE_ALL_AS_INDEXED_ACCESS_TYPES: u32 = 95034;
    pub const CONVERT_ALL_TO_DEFAULT_IMPORTS: u32 = 95035;
    pub const MAKE_ALL_SUPER_CALLS_THE_FIRST_STATEMENT_IN_THEIR_CONSTRUCTOR: u32 = 95036;
    pub const ADD_QUALIFIER_TO_ALL_UNRESOLVED_VARIABLES_MATCHING_A_MEMBER_NAME: u32 = 95037;
    pub const CHANGE_ALL_EXTENDED_INTERFACES_TO_IMPLEMENTS: u32 = 95038;
    pub const ADD_ALL_MISSING_SUPER_CALLS: u32 = 95039;
    pub const IMPLEMENT_ALL_INHERITED_ABSTRACT_CLASSES: u32 = 95040;
    pub const ADD_ALL_MISSING_ASYNC_MODIFIERS: u32 = 95041;
    pub const ADD_TS_IGNORE_TO_ALL_ERROR_MESSAGES: u32 = 95042;
    pub const ANNOTATE_EVERYTHING_WITH_TYPES_FROM_JSDOC: u32 = 95043;
    pub const ADD_TO_ALL_UNCALLED_DECORATORS: u32 = 95044;
    pub const CONVERT_ALL_CONSTRUCTOR_FUNCTIONS_TO_CLASSES: u32 = 95045;
    pub const GENERATE_GET_AND_SET_ACCESSORS: u32 = 95046;
    pub const CONVERT_REQUIRE_TO_IMPORT: u32 = 95047;
    pub const CONVERT_ALL_REQUIRE_TO_IMPORT: u32 = 95048;
    pub const MOVE_TO_A_NEW_FILE: u32 = 95049;
    pub const REMOVE_UNREACHABLE_CODE: u32 = 95050;
    pub const REMOVE_ALL_UNREACHABLE_CODE: u32 = 95051;
    pub const ADD_MISSING_TYPEOF: u32 = 95052;
    pub const REMOVE_UNUSED_LABEL: u32 = 95053;
    pub const REMOVE_ALL_UNUSED_LABELS: u32 = 95054;
    pub const CONVERT_TO_MAPPED_OBJECT_TYPE: u32 = 95055;
    pub const CONVERT_NAMESPACE_IMPORT_TO_NAMED_IMPORTS: u32 = 95056;
    pub const CONVERT_NAMED_IMPORTS_TO_NAMESPACE_IMPORT: u32 = 95057;
    pub const ADD_OR_REMOVE_BRACES_IN_AN_ARROW_FUNCTION: u32 = 95058;
    pub const ADD_BRACES_TO_ARROW_FUNCTION: u32 = 95059;
    pub const REMOVE_BRACES_FROM_ARROW_FUNCTION: u32 = 95060;
    pub const CONVERT_DEFAULT_EXPORT_TO_NAMED_EXPORT: u32 = 95061;
    pub const CONVERT_NAMED_EXPORT_TO_DEFAULT_EXPORT: u32 = 95062;
    pub const ADD_MISSING_ENUM_MEMBER: u32 = 95063;
    pub const ADD_ALL_MISSING_IMPORTS: u32 = 95064;
    pub const CONVERT_TO_ASYNC_FUNCTION: u32 = 95065;
    pub const CONVERT_ALL_TO_ASYNC_FUNCTIONS: u32 = 95066;
    pub const ADD_MISSING_CALL_PARENTHESES: u32 = 95067;
    pub const ADD_ALL_MISSING_CALL_PARENTHESES: u32 = 95068;
    pub const ADD_UNKNOWN_CONVERSION_FOR_NON_OVERLAPPING_TYPES: u32 = 95069;
    pub const ADD_UNKNOWN_TO_ALL_CONVERSIONS_OF_NON_OVERLAPPING_TYPES: u32 = 95070;
    pub const ADD_MISSING_NEW_OPERATOR_TO_CALL: u32 = 95071;
    pub const ADD_MISSING_NEW_OPERATOR_TO_ALL_CALLS: u32 = 95072;
    pub const ADD_NAMES_TO_ALL_PARAMETERS_WITHOUT_NAMES: u32 = 95073;
    pub const ENABLE_THE_EXPERIMENTALDECORATORS_OPTION_IN_YOUR_CONFIGURATION_FILE: u32 = 95074;
    pub const CONVERT_PARAMETERS_TO_DESTRUCTURED_OBJECT: u32 = 95075;
    pub const EXTRACT_TYPE: u32 = 95077;
    pub const EXTRACT_TO_TYPE_ALIAS: u32 = 95078;
    pub const EXTRACT_TO_TYPEDEF: u32 = 95079;
    pub const INFER_THIS_TYPE_OF_FROM_USAGE: u32 = 95080;
    pub const ADD_CONST_TO_UNRESOLVED_VARIABLE: u32 = 95081;
    pub const ADD_CONST_TO_ALL_UNRESOLVED_VARIABLES: u32 = 95082;
    pub const ADD_AWAIT: u32 = 95083;
    pub const ADD_AWAIT_TO_INITIALIZER_FOR: u32 = 95084;
    pub const FIX_ALL_EXPRESSIONS_POSSIBLY_MISSING_AWAIT: u32 = 95085;
    pub const REMOVE_UNNECESSARY_AWAIT: u32 = 95086;
    pub const REMOVE_ALL_UNNECESSARY_USES_OF_AWAIT: u32 = 95087;
    pub const ENABLE_THE_JSX_FLAG_IN_YOUR_CONFIGURATION_FILE: u32 = 95088;
    pub const ADD_AWAIT_TO_INITIALIZERS: u32 = 95089;
    pub const EXTRACT_TO_INTERFACE: u32 = 95090;
    pub const CONVERT_TO_A_BIGINT_NUMERIC_LITERAL: u32 = 95091;
    pub const CONVERT_ALL_TO_BIGINT_NUMERIC_LITERALS: u32 = 95092;
    pub const CONVERT_CONST_TO_LET: u32 = 95093;
    pub const PREFIX_WITH_DECLARE: u32 = 95094;
    pub const PREFIX_ALL_INCORRECT_PROPERTY_DECLARATIONS_WITH_DECLARE: u32 = 95095;
    pub const CONVERT_TO_TEMPLATE_STRING: u32 = 95096;
    pub const ADD_EXPORT_TO_MAKE_THIS_FILE_INTO_A_MODULE: u32 = 95097;
    pub const SET_THE_TARGET_OPTION_IN_YOUR_CONFIGURATION_FILE_TO: u32 = 95098;
    pub const SET_THE_MODULE_OPTION_IN_YOUR_CONFIGURATION_FILE_TO: u32 = 95099;
    pub const CONVERT_INVALID_CHARACTER_TO_ITS_HTML_ENTITY_CODE: u32 = 95100;
    pub const CONVERT_ALL_INVALID_CHARACTERS_TO_HTML_ENTITY_CODE: u32 = 95101;
    pub const CONVERT_ALL_CONST_TO_LET: u32 = 95102;
    pub const CONVERT_FUNCTION_EXPRESSION_TO_ARROW_FUNCTION: u32 = 95105;
    pub const CONVERT_FUNCTION_DECLARATION_TO_ARROW_FUNCTION: u32 = 95106;
    pub const FIX_ALL_IMPLICIT_THIS_ERRORS: u32 = 95107;
    pub const WRAP_INVALID_CHARACTER_IN_AN_EXPRESSION_CONTAINER: u32 = 95108;
    pub const WRAP_ALL_INVALID_CHARACTERS_IN_AN_EXPRESSION_CONTAINER: u32 = 95109;
    pub const VISIT_HTTPS_AKA_MS_TSCONFIG_TO_READ_MORE_ABOUT_THIS_FILE: u32 = 95110;
    pub const ADD_A_RETURN_STATEMENT: u32 = 95111;
    pub const REMOVE_BRACES_FROM_ARROW_FUNCTION_BODY: u32 = 95112;
    pub const WRAP_THE_FOLLOWING_BODY_WITH_PARENTHESES_WHICH_SHOULD_BE_AN_OBJECT_LITERAL: u32 =
        95113;
    pub const ADD_ALL_MISSING_RETURN_STATEMENT: u32 = 95114;
    pub const REMOVE_BRACES_FROM_ALL_ARROW_FUNCTION_BODIES_WITH_RELEVANT_ISSUES: u32 = 95115;
    pub const WRAP_ALL_OBJECT_LITERAL_WITH_PARENTHESES: u32 = 95116;
    pub const MOVE_LABELED_TUPLE_ELEMENT_MODIFIERS_TO_LABELS: u32 = 95117;
    pub const CONVERT_OVERLOAD_LIST_TO_SINGLE_SIGNATURE: u32 = 95118;
    pub const GENERATE_GET_AND_SET_ACCESSORS_FOR_ALL_OVERRIDING_PROPERTIES: u32 = 95119;
    pub const WRAP_IN_JSX_FRAGMENT: u32 = 95120;
    pub const WRAP_ALL_UNPARENTED_JSX_IN_JSX_FRAGMENT: u32 = 95121;
    pub const CONVERT_ARROW_FUNCTION_OR_FUNCTION_EXPRESSION: u32 = 95122;
    pub const CONVERT_TO_ANONYMOUS_FUNCTION: u32 = 95123;
    pub const CONVERT_TO_NAMED_FUNCTION: u32 = 95124;
    pub const CONVERT_TO_ARROW_FUNCTION: u32 = 95125;
    pub const REMOVE_PARENTHESES: u32 = 95126;
    pub const COULD_NOT_FIND_A_CONTAINING_ARROW_FUNCTION: u32 = 95127;
    pub const CONTAINING_FUNCTION_IS_NOT_AN_ARROW_FUNCTION: u32 = 95128;
    pub const COULD_NOT_FIND_EXPORT_STATEMENT: u32 = 95129;
    pub const THIS_FILE_ALREADY_HAS_A_DEFAULT_EXPORT: u32 = 95130;
    pub const COULD_NOT_FIND_IMPORT_CLAUSE: u32 = 95131;
    pub const COULD_NOT_FIND_NAMESPACE_IMPORT_OR_NAMED_IMPORTS: u32 = 95132;
    pub const SELECTION_IS_NOT_A_VALID_TYPE_NODE: u32 = 95133;
    pub const NO_TYPE_COULD_BE_EXTRACTED_FROM_THIS_TYPE_NODE: u32 = 95134;
    pub const COULD_NOT_FIND_PROPERTY_FOR_WHICH_TO_GENERATE_ACCESSOR: u32 = 95135;
    pub const NAME_IS_NOT_VALID: u32 = 95136;
    pub const CAN_ONLY_CONVERT_PROPERTY_WITH_MODIFIER: u32 = 95137;
    pub const SWITCH_EACH_MISUSED_TO: u32 = 95138;
    pub const CONVERT_TO_OPTIONAL_CHAIN_EXPRESSION: u32 = 95139;
    pub const COULD_NOT_FIND_CONVERTIBLE_ACCESS_EXPRESSION: u32 = 95140;
    pub const COULD_NOT_FIND_MATCHING_ACCESS_EXPRESSIONS: u32 = 95141;
    pub const CAN_ONLY_CONVERT_LOGICAL_AND_ACCESS_CHAINS: u32 = 95142;
    pub const ADD_VOID_TO_PROMISE_RESOLVED_WITHOUT_A_VALUE: u32 = 95143;
    pub const ADD_VOID_TO_ALL_PROMISES_RESOLVED_WITHOUT_A_VALUE: u32 = 95144;
    pub const USE_ELEMENT_ACCESS_FOR: u32 = 95145;
    pub const USE_ELEMENT_ACCESS_FOR_ALL_UNDECLARED_PROPERTIES: u32 = 95146;
    pub const DELETE_ALL_UNUSED_IMPORTS: u32 = 95147;
    pub const INFER_FUNCTION_RETURN_TYPE: u32 = 95148;
    pub const RETURN_TYPE_MUST_BE_INFERRED_FROM_A_FUNCTION: u32 = 95149;
    pub const COULD_NOT_DETERMINE_FUNCTION_RETURN_TYPE: u32 = 95150;
    pub const COULD_NOT_CONVERT_TO_ARROW_FUNCTION: u32 = 95151;
    pub const COULD_NOT_CONVERT_TO_NAMED_FUNCTION: u32 = 95152;
    pub const COULD_NOT_CONVERT_TO_ANONYMOUS_FUNCTION: u32 = 95153;
    pub const CAN_ONLY_CONVERT_STRING_CONCATENATIONS_AND_STRING_LITERALS: u32 = 95154;
    pub const SELECTION_IS_NOT_A_VALID_STATEMENT_OR_STATEMENTS: u32 = 95155;
    pub const ADD_MISSING_FUNCTION_DECLARATION: u32 = 95156;
    pub const ADD_ALL_MISSING_FUNCTION_DECLARATIONS: u32 = 95157;
    pub const METHOD_NOT_IMPLEMENTED: u32 = 95158;
    pub const FUNCTION_NOT_IMPLEMENTED: u32 = 95159;
    pub const ADD_OVERRIDE_MODIFIER: u32 = 95160;
    pub const REMOVE_OVERRIDE_MODIFIER: u32 = 95161;
    pub const ADD_ALL_MISSING_OVERRIDE_MODIFIERS: u32 = 95162;
    pub const REMOVE_ALL_UNNECESSARY_OVERRIDE_MODIFIERS: u32 = 95163;
    pub const CAN_ONLY_CONVERT_NAMED_EXPORT: u32 = 95164;
    pub const ADD_MISSING_PROPERTIES: u32 = 95165;
    pub const ADD_ALL_MISSING_PROPERTIES: u32 = 95166;
    pub const ADD_MISSING_ATTRIBUTES: u32 = 95167;
    pub const ADD_ALL_MISSING_ATTRIBUTES: u32 = 95168;
    pub const ADD_UNDEFINED_TO_OPTIONAL_PROPERTY_TYPE: u32 = 95169;
    pub const CONVERT_NAMED_IMPORTS_TO_DEFAULT_IMPORT: u32 = 95170;
    pub const DELETE_UNUSED_PARAM_TAG: u32 = 95171;
    pub const DELETE_ALL_UNUSED_PARAM_TAGS: u32 = 95172;
    pub const RENAME_PARAM_TAG_NAME_TO: u32 = 95173;
    pub const USE: u32 = 95174;
    pub const USE_NUMBER_ISNAN_IN_ALL_CONDITIONS: u32 = 95175;
    pub const CONVERT_TYPEDEF_TO_TYPESCRIPT_TYPE: u32 = 95176;
    pub const CONVERT_ALL_TYPEDEF_TO_TYPESCRIPT_TYPES: u32 = 95177;
    pub const MOVE_TO_FILE: u32 = 95178;
    pub const CANNOT_MOVE_TO_FILE_SELECTED_FILE_IS_INVALID: u32 = 95179;
    pub const USE_IMPORT_TYPE: u32 = 95180;
    pub const USE_TYPE: u32 = 95181;
    pub const FIX_ALL_WITH_TYPE_ONLY_IMPORTS: u32 = 95182;
    pub const CANNOT_MOVE_STATEMENTS_TO_THE_SELECTED_FILE: u32 = 95183;
    pub const INLINE_VARIABLE: u32 = 95184;
    pub const COULD_NOT_FIND_VARIABLE_TO_INLINE: u32 = 95185;
    pub const VARIABLES_WITH_MULTIPLE_DECLARATIONS_CANNOT_BE_INLINED: u32 = 95186;
    pub const ADD_MISSING_COMMA_FOR_OBJECT_MEMBER_COMPLETION: u32 = 95187;
    pub const ADD_MISSING_PARAMETER_TO: u32 = 95188;
    pub const ADD_MISSING_PARAMETERS_TO: u32 = 95189;
    pub const ADD_ALL_MISSING_PARAMETERS: u32 = 95190;
    pub const ADD_OPTIONAL_PARAMETER_TO: u32 = 95191;
    pub const ADD_OPTIONAL_PARAMETERS_TO: u32 = 95192;
    pub const ADD_ALL_OPTIONAL_PARAMETERS: u32 = 95193;
    pub const WRAP_IN_PARENTHESES: u32 = 95194;
    pub const WRAP_ALL_INVALID_DECORATOR_EXPRESSIONS_IN_PARENTHESES: u32 = 95195;
    pub const ADD_RESOLUTION_MODE_IMPORT_ATTRIBUTE: u32 = 95196;
    pub const ADD_RESOLUTION_MODE_IMPORT_ATTRIBUTE_TO_ALL_TYPE_ONLY_IMPORTS_THAT_NEED_IT: u32 =
        95197;
    pub const DUPLICATE_IDENTIFIER: u32 = 2300;
}
