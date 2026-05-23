//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
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
