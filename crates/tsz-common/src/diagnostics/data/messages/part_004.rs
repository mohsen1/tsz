//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
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
];
