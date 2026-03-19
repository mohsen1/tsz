//! Custom help renderer matching tsc v6 output exactly.

/// The version string embedded at build time from `scripts/conformance/typescript-versions.json`.
pub const TSC_VERSION: &str = env!("TSZ_TSC_VERSION");

/// Render the standard `--help` output, matching `tsc --help` exactly.
pub fn render_help(version: &str) -> String {
    format!(
        "\
tsc: The TypeScript Compiler - Version {version}

COMMON COMMANDS

  tsc
  Compiles the current project (tsconfig.json in the working directory.)

  tsc app.ts util.ts
  Ignoring tsconfig.json, compiles the specified files with default compiler options.

  tsc -b
  Build a composite project in the working directory.

  tsc --init
  Creates a tsconfig.json with the recommended settings in the working directory.

  tsc -p ./path/to/tsconfig.json
  Compiles the TypeScript project located at the specified path.

  tsc --help --all
  An expanded version of this information, showing all possible compiler options

  tsc --noEmit
  tsc --target esnext
  Compiles the current project, with additional settings.

COMMAND LINE FLAGS

--help, -h
Print this message.

--watch, -w
Watch input files.

--all
Show all compiler options.

--version, -v
Print the compiler's version.

--init
Initializes a TypeScript project and creates a tsconfig.json file.

--project, -p
Compile the project given the path to its configuration file, or to a folder with a 'tsconfig.json'.

--showConfig
Print the final configuration instead of building.

--ignoreConfig
Ignore the tsconfig found and build with commandline options and files.

--build, -b
Build one or more projects and their dependencies, if out of date

COMMON COMPILER OPTIONS

--pretty
Enable color and formatting in TypeScript's output to make compiler errors easier to read.
type: boolean
default: true

--declaration, -d
Generate .d.ts files from TypeScript and JavaScript files in your project.
type: boolean
default: `false`, unless `composite` is set

--declarationMap
Create sourcemaps for d.ts files.
type: boolean
default: false

--emitDeclarationOnly
Only output d.ts files and not JavaScript files.
type: boolean
default: false

--sourceMap
Create source map files for emitted JavaScript files.
type: boolean
default: false

--noEmit
Disable emitting files from a compilation.
type: boolean
default: false

--target, -t
Set the JavaScript language version for emitted JavaScript and include compatible library declarations.
one of: es6/es2015, es2016, es2017, es2018, es2019, es2020, es2021, es2022, es2023, es2024, es2025, esnext
default: es2025

--module, -m
Specify what module code is generated.
one of: commonjs, es6/es2015, es2020, es2022, esnext, node16, node18, node20, nodenext, preserve
default: undefined

--lib
Specify a set of bundled library declaration files that describe the target runtime environment.
one or more: es5, es6/es2015, es7/es2016, es2017, es2018, es2019, es2020, es2021, es2022, es2023, es2024, es2025, esnext, dom, dom.iterable, dom.asynciterable, webworker, webworker.importscripts, webworker.iterable, webworker.asynciterable, scripthost, es2015.core, es2015.collection, es2015.generator, es2015.iterable, es2015.promise, es2015.proxy, es2015.reflect, es2015.symbol, es2015.symbol.wellknown, es2016.array.include, es2016.intl, es2017.arraybuffer, es2017.date, es2017.object, es2017.sharedmemory, es2017.string, es2017.intl, es2017.typedarrays, es2018.asyncgenerator, es2018.asynciterable/esnext.asynciterable, es2018.intl, es2018.promise, es2018.regexp, es2019.array, es2019.object, es2019.string, es2019.symbol/esnext.symbol, es2019.intl, es2020.bigint/esnext.bigint, es2020.date, es2020.promise, es2020.sharedmemory, es2020.string, es2020.symbol.wellknown, es2020.intl, es2020.number, es2021.promise, es2021.string, es2021.weakref/esnext.weakref, es2021.intl, es2022.array, es2022.error, es2022.intl, es2022.object, es2022.string, es2022.regexp, es2023.array, es2023.collection, es2023.intl, es2024.arraybuffer, es2024.collection, es2024.object/esnext.object, es2024.promise, es2024.regexp/esnext.regexp, es2024.sharedmemory, es2024.string/esnext.string, es2025.collection, es2025.float16/esnext.float16, es2025.intl, es2025.iterator/esnext.iterator, es2025.promise/esnext.promise, es2025.regexp, esnext.array, esnext.collection, esnext.date, esnext.decorators, esnext.disposable, esnext.error, esnext.intl, esnext.sharedmemory, esnext.temporal, esnext.typedarrays, decorators, decorators.legacy
default: undefined

--allowJs
Allow JavaScript files to be a part of your program. Use the 'checkJs' option to get errors from these files.
type: boolean
default: `false`, unless `checkJs` is set

--checkJs
Enable error reporting in type-checked JavaScript files.
type: boolean
default: false

--jsx
Specify what JSX code is generated.
one of: preserve, react, react-native, react-jsx, react-jsxdev
default: undefined

--outFile
Specify a file that bundles all outputs into one JavaScript file. If 'declaration' is true, also designates a file that bundles all .d.ts output.

--outDir
Specify an output folder for all emitted files.

--removeComments
Disable emitting comments.
type: boolean
default: false

--strict
Enable all strict type-checking options.
type: boolean
default: true

--types
Specify type package names to be included without being referenced in a source file.

--esModuleInterop
Emit additional JavaScript to ease support for importing CommonJS modules. This enables 'allowSyntheticDefaultImports' for type compatibility.
type: boolean
default: true

You can learn about all of the compiler options at https://aka.ms/tsc
"
    )
}

/// Render the `--help --all` output, matching `tsc --help --all` exactly.
pub fn render_help_all(version: &str) -> String {
    format!(
        "\
tsc: The TypeScript Compiler - Version {version}

ALL COMPILER OPTIONS

### Command-line Options

--all
Show all compiler options.

--build, -b
Build one or more projects and their dependencies, if out of date

--help, -h
Print this message.

--help, -?


--ignoreConfig
Ignore the tsconfig found and build with commandline options and files.

--init
Initializes a TypeScript project and creates a tsconfig.json file.

--listFilesOnly
Print names of files that are part of the compilation and then stop processing.

--locale
Set the language of the messaging from TypeScript. This does not affect emit.

--project, -p
Compile the project given the path to its configuration file, or to a folder with a 'tsconfig.json'.

--showConfig
Print the final configuration instead of building.

--version, -v
Print the compiler's version.

--watch, -w
Watch input files.

### Modules

--allowArbitraryExtensions
Enable importing files with any extension, provided a declaration file is present.
type: boolean
default: false

--allowImportingTsExtensions
Allow imports to include TypeScript file extensions. Requires '--moduleResolution bundler' and either '--noEmit' or '--emitDeclarationOnly' to be set.
type: boolean
default: false

--allowUmdGlobalAccess
Allow accessing UMD globals from modules.
type: boolean
default: false

--baseUrl
Specify the base directory to resolve non-relative module names.

--customConditions
Conditions to set in addition to the resolver-specific defaults when resolving imports.

--module, -m
Specify what module code is generated.
one of: commonjs, es6/es2015, es2020, es2022, esnext, node16, node18, node20, nodenext, preserve
default: undefined

--moduleResolution
Specify how TypeScript looks up a file from a given module specifier.
one of: node16, nodenext, bundler
default: `nodenext` if `module` is `nodenext`; `node16` if `module` is `node16` or `node18`; otherwise, `bundler`.

--moduleSuffixes
List of file name suffixes to search when resolving a module.

--noResolve
Disallow 'import's, 'require's or '<reference>'s from expanding the number of files TypeScript should add to a project.
type: boolean
default: false

--noUncheckedSideEffectImports
Check side effect imports.
type: boolean
default: true

--paths
Specify a set of entries that re-map imports to additional lookup locations.
default: undefined

--resolveJsonModule
Enable importing .json files.
type: boolean
default: false

--resolvePackageJsonExports
Use the package.json 'exports' field when resolving package imports.
type: boolean
default: `true` when 'moduleResolution' is 'node16', 'nodenext', or 'bundler'; otherwise `false`.

--resolvePackageJsonImports
Use the package.json 'imports' field when resolving imports.
type: boolean
default: `true` when 'moduleResolution' is 'node16', 'nodenext', or 'bundler'; otherwise `false`.

--rewriteRelativeImportExtensions
Rewrite '.ts', '.tsx', '.mts', and '.cts' file extensions in relative import paths to their JavaScript equivalent in output files.
type: boolean
default: false

--rootDir
Specify the root folder within your source files.
type: string
default: Computed from the list of input files

--rootDirs
Allow multiple folders to be treated as one when resolving modules.
one or more: string
default: Computed from the list of input files

--typeRoots
Specify multiple folders that act like './node_modules/@types'.

--types
Specify type package names to be included without being referenced in a source file.

### JavaScript Support

--allowJs
Allow JavaScript files to be a part of your program. Use the 'checkJs' option to get errors from these files.
type: boolean
default: `false`, unless `checkJs` is set

--checkJs
Enable error reporting in type-checked JavaScript files.
type: boolean
default: false

--maxNodeModuleJsDepth
Specify the maximum folder depth used for checking JavaScript files from 'node_modules'. Only applicable with 'allowJs'.
type: number
default: 0

### Interop Constraints

--allowSyntheticDefaultImports
Allow 'import x from y' when a module doesn't have a default export.
type: boolean
default: true

--erasableSyntaxOnly
Do not allow runtime constructs that are not part of ECMAScript.
type: boolean
default: false

--esModuleInterop
Emit additional JavaScript to ease support for importing CommonJS modules. This enables 'allowSyntheticDefaultImports' for type compatibility.
type: boolean
default: true

--forceConsistentCasingInFileNames
Ensure that casing is correct in imports.
type: boolean
default: true

--isolatedDeclarations
Require sufficient annotation on exports so other tools can trivially generate declaration files.
type: boolean
default: false

--isolatedModules
Ensure that each file can be safely transpiled without relying on other imports.
type: boolean
default: false

--preserveSymlinks
Disable resolving symlinks to their realpath. This correlates to the same flag in node.
type: boolean
default: false

--verbatimModuleSyntax
Do not transform or elide any imports or exports not marked as type-only, ensuring they are written in the output file's format based on the 'module' setting.
type: boolean
default: false

### Type Checking

--allowUnreachableCode
Disable error reporting for unreachable code.
type: boolean
default: undefined

--allowUnusedLabels
Disable error reporting for unused labels.
type: boolean
default: undefined

--alwaysStrict
Ensure 'use strict' is always emitted.
type: boolean
default: true

--exactOptionalPropertyTypes
Interpret optional property types as written, rather than adding 'undefined'.
type: boolean
default: false

--noFallthroughCasesInSwitch
Enable error reporting for fallthrough cases in switch statements.
type: boolean
default: false

--noImplicitAny
Enable error reporting for expressions and declarations with an implied 'any' type.
type: boolean
default: `true`, unless `strict` is `false`

--noImplicitOverride
Ensure overriding members in derived classes are marked with an override modifier.
type: boolean
default: false

--noImplicitReturns
Enable error reporting for codepaths that do not explicitly return in a function.
type: boolean
default: false

--noImplicitThis
Enable error reporting when 'this' is given the type 'any'.
type: boolean
default: `true`, unless `strict` is `false`

--noPropertyAccessFromIndexSignature
Enforces using indexed accessors for keys declared using an indexed type.
type: boolean
default: false

--noUncheckedIndexedAccess
Add 'undefined' to a type when accessed using an index.
type: boolean
default: false

--noUnusedLocals
Enable error reporting when local variables aren't read.
type: boolean
default: false

--noUnusedParameters
Raise an error when a function parameter isn't read.
type: boolean
default: false

--strict
Enable all strict type-checking options.
type: boolean
default: true

--strictBindCallApply
Check that the arguments for 'bind', 'call', and 'apply' methods match the original function.
type: boolean
default: `true`, unless `strict` is `false`

--strictBuiltinIteratorReturn
Built-in iterators are instantiated with a 'TReturn' type of 'undefined' instead of 'any'.
type: boolean
default: `true`, unless `strict` is `false`

--strictFunctionTypes
When assigning functions, check to ensure parameters and the return values are subtype-compatible.
type: boolean
default: `true`, unless `strict` is `false`

--strictNullChecks
When type checking, take into account 'null' and 'undefined'.
type: boolean
default: `true`, unless `strict` is `false`

--strictPropertyInitialization
Check for class properties that are declared but not set in the constructor.
type: boolean
default: `true`, unless `strict` is `false`

--useUnknownInCatchVariables
Default catch clause variables as 'unknown' instead of 'any'.
type: boolean
default: `true`, unless `strict` is `false`

### Watch and Build Modes

--assumeChangesOnlyAffectDirectDependencies
Have recompiles in projects that use 'incremental' and 'watch' mode assume that changes within a file will only affect files directly depending on it.
type: boolean
default: false

### Backwards Compatibility

--charset
No longer supported. In early versions, manually set the text encoding for reading files.
type: string
default: utf8

--importsNotUsedAsValues
Specify emit/checking behavior for imports that are only used for types.
one of: remove, preserve, error
default: remove

--keyofStringsOnly
Make keyof only return strings instead of string, numbers or symbols. Legacy option.
type: boolean
default: false

--noImplicitUseStrict
Disable adding 'use strict' directives in emitted JavaScript files.
type: boolean
default: false

--noStrictGenericChecks
Disable strict checking of generic signatures in function types.
type: boolean
default: false

--out
Deprecated setting. Use 'outFile' instead.

--preserveValueImports
Preserve unused imported values in the JavaScript output that would otherwise be removed.
type: boolean
default: false

--suppressExcessPropertyErrors
Disable reporting of excess property errors during the creation of object literals.
type: boolean
default: false

--suppressImplicitAnyIndexErrors
Suppress 'noImplicitAny' errors when indexing objects that lack index signatures.
type: boolean
default: false

### Projects

--composite
Enable constraints that allow a TypeScript project to be used with project references.
type: boolean
default: false

--disableReferencedProjectLoad
Reduce the number of projects loaded automatically by TypeScript.
type: boolean
default: false

--disableSolutionSearching
Opt a project out of multi-project reference checking when editing.
type: boolean
default: false

--disableSourceOfProjectReferenceRedirect
Disable preferring source files instead of declaration files when referencing composite projects.
type: boolean
default: false

--incremental, -i
Save .tsbuildinfo files to allow for incremental compilation of projects.
type: boolean
default: `false`, unless `composite` is set

--tsBuildInfoFile
Specify the path to .tsbuildinfo incremental compilation file.
type: string
default: .tsbuildinfo

### Emit

--declaration, -d
Generate .d.ts files from TypeScript and JavaScript files in your project.
type: boolean
default: `false`, unless `composite` is set

--declarationDir
Specify the output directory for generated declaration files.

--declarationMap
Create sourcemaps for d.ts files.
type: boolean
default: false

--downlevelIteration
Emit more compliant, but verbose and less performant JavaScript for iteration.
type: boolean
default: false

--emitBOM
Emit a UTF-8 Byte Order Mark (BOM) in the beginning of output files.
type: boolean
default: false

--emitDeclarationOnly
Only output d.ts files and not JavaScript files.
type: boolean
default: false

--importHelpers
Allow importing helper functions from tslib once per project, instead of including them per-file.
type: boolean
default: false

--inlineSourceMap
Include sourcemap files inside the emitted JavaScript.
type: boolean
default: false

--inlineSources
Include source code in the sourcemaps inside the emitted JavaScript.
type: boolean
default: false

--mapRoot
Specify the location where debugger should locate map files instead of generated locations.

--newLine
Set the newline character for emitting files.
one of: crlf, lf

--noEmit
Disable emitting files from a compilation.
type: boolean
default: false

--noEmitHelpers
Disable generating custom helper functions like '__extends' in compiled output.
type: boolean
default: false

--noEmitOnError
Disable emitting files if any type checking errors are reported.
type: boolean
default: false

--outDir
Specify an output folder for all emitted files.

--outFile
Specify a file that bundles all outputs into one JavaScript file. If 'declaration' is true, also designates a file that bundles all .d.ts output.

--preserveConstEnums
Disable erasing 'const enum' declarations in generated code.
type: boolean
default: false

--removeComments
Disable emitting comments.
type: boolean
default: false

--sourceMap
Create source map files for emitted JavaScript files.
type: boolean
default: false

--sourceRoot
Specify the root path for debuggers to find the reference source code.

--stripInternal
Disable emitting declarations that have '@internal' in their JSDoc comments.
type: boolean
default: false

### Compiler Diagnostics

--diagnostics
Output compiler performance information after building.
type: boolean
default: false

--explainFiles
Print files read during the compilation including why it was included.
type: boolean
default: false

--extendedDiagnostics
Output more detailed compiler performance information after building.
type: boolean
default: false

--generateCpuProfile
Emit a v8 CPU profile of the compiler run for debugging.
type: string
default: profile.cpuprofile

--generateTrace
Generates an event trace and a list of types.

--listEmittedFiles
Print the names of emitted files after a compilation.
type: boolean
default: false

--listFiles
Print all of the files read during the compilation.
type: boolean
default: false

--noCheck
Disable full type checking (only critical parse and emit errors will be reported).
type: boolean
default: false

--traceResolution
Log paths used during the 'moduleResolution' process.
type: boolean
default: false

### Editor Support

--disableSizeLimit
Remove the 20mb cap on total source code size for JavaScript files in the TypeScript language server.
type: boolean
default: false

--plugins
Specify a list of language service plugins to include.
one or more: 
default: undefined

### Language and Environment

--emitDecoratorMetadata
Emit design-type metadata for decorated declarations in source files.
type: boolean
default: false

--experimentalDecorators
Enable experimental support for legacy experimental decorators.
type: boolean
default: false

--jsx
Specify what JSX code is generated.
one of: preserve, react, react-native, react-jsx, react-jsxdev
default: undefined

--jsxFactory
Specify the JSX factory function used when targeting React JSX emit, e.g. 'React.createElement' or 'h'.
type: string
default: `React.createElement`

--jsxFragmentFactory
Specify the JSX Fragment reference used for fragments when targeting React JSX emit e.g. 'React.Fragment' or 'Fragment'.
type: string
default: React.Fragment

--jsxImportSource
Specify module specifier used to import the JSX factory functions when using 'jsx: react-jsx*'.
type: string
default: react

--lib
Specify a set of bundled library declaration files that describe the target runtime environment.
one or more: es5, es6/es2015, es7/es2016, es2017, es2018, es2019, es2020, es2021, es2022, es2023, es2024, es2025, esnext, dom, dom.iterable, dom.asynciterable, webworker, webworker.importscripts, webworker.iterable, webworker.asynciterable, scripthost, es2015.core, es2015.collection, es2015.generator, es2015.iterable, es2015.promise, es2015.proxy, es2015.reflect, es2015.symbol, es2015.symbol.wellknown, es2016.array.include, es2016.intl, es2017.arraybuffer, es2017.date, es2017.object, es2017.sharedmemory, es2017.string, es2017.intl, es2017.typedarrays, es2018.asyncgenerator, es2018.asynciterable/esnext.asynciterable, es2018.intl, es2018.promise, es2018.regexp, es2019.array, es2019.object, es2019.string, es2019.symbol/esnext.symbol, es2019.intl, es2020.bigint/esnext.bigint, es2020.date, es2020.promise, es2020.sharedmemory, es2020.string, es2020.symbol.wellknown, es2020.intl, es2020.number, es2021.promise, es2021.string, es2021.weakref/esnext.weakref, es2021.intl, es2022.array, es2022.error, es2022.intl, es2022.object, es2022.string, es2022.regexp, es2023.array, es2023.collection, es2023.intl, es2024.arraybuffer, es2024.collection, es2024.object/esnext.object, es2024.promise, es2024.regexp/esnext.regexp, es2024.sharedmemory, es2024.string/esnext.string, es2025.collection, es2025.float16/esnext.float16, es2025.intl, es2025.iterator/esnext.iterator, es2025.promise/esnext.promise, es2025.regexp, esnext.array, esnext.collection, esnext.date, esnext.decorators, esnext.disposable, esnext.error, esnext.intl, esnext.sharedmemory, esnext.temporal, esnext.typedarrays, decorators, decorators.legacy
default: undefined

--libReplacement
Enable lib replacement.
type: boolean
default: false

--moduleDetection
Control what method is used to detect module-format JS files.
one of: legacy, auto, force
default: \"auto\": Treat files with imports, exports, import.meta, jsx (with jsx: react-jsx), or esm format (with module: node16+) as modules.

--noLib
Disable including any library files, including the default lib.d.ts.
type: boolean
default: false

--reactNamespace
Specify the object invoked for 'createElement'. This only applies when targeting 'react' JSX emit.
type: string
default: `React`

--target, -t
Set the JavaScript language version for emitted JavaScript and include compatible library declarations.
one of: es6/es2015, es2016, es2017, es2018, es2019, es2020, es2021, es2022, es2023, es2024, es2025, esnext
default: es2025

--useDefineForClassFields
Emit ECMAScript-standard-compliant class fields.
type: boolean
default: `true` for ES2022 and above, including ESNext.

### Output Formatting

--noErrorTruncation
Disable truncating types in error messages.
type: boolean
default: false

--preserveWatchOutput
Disable wiping the console in watch mode.
type: boolean
default: false

--pretty
Enable color and formatting in TypeScript's output to make compiler errors easier to read.
type: boolean
default: true

### Completeness

--skipDefaultLibCheck
Skip type checking .d.ts files that are included with TypeScript.
type: boolean
default: false

--skipLibCheck
Skip type checking all .d.ts files.
type: boolean
default: false

You can learn about all of the compiler options at https://aka.ms/tsc

WATCH OPTIONS

Including --watch, -w will start watching the current project for the file changes. Once set, you can config watch mode with:

--watchFile
Specify how the TypeScript watch mode works.
one of: fixedpollinginterval, prioritypollinginterval, dynamicprioritypolling, fixedchunksizepolling, usefsevents, usefseventsonparentdirectory
default: usefsevents

--watchDirectory
Specify how directories are watched on systems that lack recursive file-watching functionality.
one of: usefsevents, fixedpollinginterval, dynamicprioritypolling, fixedchunksizepolling
default: usefsevents

--fallbackPolling
Specify what approach the watcher should use if the system runs out of native file watchers.
one of: fixedinterval, priorityinterval, dynamicpriority, fixedchunksize
default: priorityinterval

--synchronousWatchDirectory
Synchronously call callbacks and update the state of directory watchers on platforms that don`t support recursive watching natively.
type: boolean
default: false

--excludeDirectories
Remove a list of directories from the watch process.

--excludeFiles
Remove a list of files from the watch mode's processing.

BUILD OPTIONS

Using --build, -b will make tsc behave more like a build orchestrator than a compiler. This is used to trigger building composite projects which you can learn more about at https://aka.ms/tsc-composite-builds

--verbose, -v
Enable verbose logging.

--dry, -d
Show what would be built (or deleted, if specified with '--clean')

--force, -f
Build all projects, including those that appear to be up to date.

--clean
Delete the outputs of all projects.

--stopBuildOnErrors
Skip building downstream projects on error in upstream project.
"
    )
}

/// Post-process help output to add ANSI color codes matching tsc v6 TTY output.
///
/// tsc v6 uses:
/// - `\x1b[1m` (bold) + `\x1b[22m` (unbold) for section headers
/// - `\x1b[94m` (bright blue) + `\x1b[39m` (reset fg) for flag names and commands
///
/// Colors are applied when `colored::control::SHOULD_COLORIZE.should_colorize()` is true
/// (i.e., stdout is a TTY or `FORCE_COLOR` is set).
pub fn colorize_help(plain: &str) -> String {
    use colored::control::SHOULD_COLORIZE;

    let force_color = std::env::var("FORCE_COLOR")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    if !force_color && !SHOULD_COLORIZE.should_colorize() {
        return plain.to_string();
    }

    let mut out = String::with_capacity(plain.len() + 256);
    for line in plain.lines() {
        if line.chars().all(|c| c.is_ascii_uppercase() || c == ' ') && !line.is_empty() {
            // Section header (all caps): bold
            out.push_str("\x1b[1m");
            out.push_str(line);
            out.push_str("\x1b[22m");
        } else if line.starts_with("--") && !line.starts_with("---") {
            // Flag name at column 0: bright blue
            out.push_str("\x1b[94m");
            out.push_str(line);
            out.push_str("\x1b[39m");
        } else if line.starts_with("  tsc") || line.starts_with("  tsc ") {
            // Command example (indented "tsc ..."): bright blue
            let indent = &line[..line.len() - line.trim_start().len()];
            let content = line.trim_start();
            out.push_str(indent);
            out.push_str("\x1b[94m");
            out.push_str(content);
            out.push_str("\x1b[39m");
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    // The lines() iterator drops the trailing newline, so we may have added
    // one extra. Match the original ending.
    if !plain.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_help_starts_with_header() {
        let output = render_help("6.0.0-dev.20260306");
        assert!(output.starts_with("tsc: The TypeScript Compiler - Version 6.0.0-dev.20260306\n"));
    }

    #[test]
    fn render_help_ends_with_footer() {
        let output = render_help("6.0.0-dev.20260306");
        assert!(
            output.ends_with(
                "You can learn about all of the compiler options at https://aka.ms/tsc\n"
            )
        );
    }

    #[test]
    fn render_help_all_starts_with_header() {
        let output = render_help_all("6.0.0-dev.20260306");
        assert!(output.starts_with("tsc: The TypeScript Compiler - Version 6.0.0-dev.20260306\n"));
    }

    #[test]
    fn render_help_all_has_watch_options() {
        let output = render_help_all("6.0.0-dev.20260306");
        assert!(output.contains("WATCH OPTIONS"));
    }

    #[test]
    fn render_help_all_has_build_options() {
        let output = render_help_all("6.0.0-dev.20260306");
        assert!(output.contains("BUILD OPTIONS"));
    }

    #[test]
    fn colorize_help_adds_bold_to_headers() {
        let plain = render_help("6.0.0-dev.20260306");
        colored::control::set_override(true);
        let colorized = colorize_help(&plain);
        assert!(colorized.contains("\x1b[1mCOMMON COMMANDS\x1b[22m"));
        assert!(colorized.contains("\x1b[94m--help, -h\x1b[39m"));
        colored::control::unset_override();
    }

    #[test]
    fn colorize_help_colors_tsc_examples_and_preserves_missing_trailing_newline() {
        colored::control::set_override(true);
        let plain = "COMMON COMMANDS\n\n  tsc app.ts\nbody";
        let colorized = colorize_help(plain);

        assert!(!colorized.ends_with('\n'));
        assert!(colorized.contains("  \x1b[94mtsc app.ts\x1b[39m"));
        assert!(colorized.contains("body"));
        colored::control::unset_override();
    }

    #[test]
    fn colorize_help_does_not_color_triple_dash_or_mixed_case_lines_as_headers() {
        colored::control::set_override(true);
        let plain = "---\nMixed Case Header\n--flag\n";
        let colorized = colorize_help(plain);

        assert!(colorized.contains("---\n"));
        assert!(colorized.contains("Mixed Case Header\n"));
        assert!(colorized.contains("\x1b[94m--flag\x1b[39m"));
        assert!(!colorized.contains("\x1b[1mMixed Case Header\x1b[22m"));
        colored::control::unset_override();
    }
}
