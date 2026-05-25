//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
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
];
