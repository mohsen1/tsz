//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
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
];
