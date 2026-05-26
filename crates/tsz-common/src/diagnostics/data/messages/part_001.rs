//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
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
];
