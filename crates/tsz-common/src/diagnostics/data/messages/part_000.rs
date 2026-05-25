//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
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
];
