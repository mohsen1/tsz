use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use crate::config::ModuleResolutionKind;
use tsz::emitter::{ModuleKind, ScriptTarget};

/// CLI arguments for the tsz binary.
#[derive(Parser, Debug)]
#[command(
    name = "tsz",
    version,
    about = "Codename Zang (Persian for rust) - TypeScript in Rust"
)]
pub struct CliArgs {
    // ==================== Command-line Only Options ====================
    /// Show all compiler options.
    #[arg(long)]
    pub all: bool,

    /// Build one or more projects and their dependencies, if out of date.
    #[arg(short = 'b', long)]
    pub build: bool,

    /// Initializes a TypeScript project and creates a tsconfig.json file.
    #[arg(long)]
    pub init: bool,

    /// Print names of files that are part of the compilation and then stop processing.
    #[arg(long = "listFilesOnly", alias = "list-files-only")]
    pub list_files_only: bool,

    /// Set the language of the messaging from TypeScript.
    #[arg(long)]
    pub locale: Option<String>,

    /// Print the final configuration instead of building.
    #[arg(long = "showConfig", alias = "show-config")]
    pub show_config: bool,

    /// Watch input files and recompile on changes.
    #[arg(short = 'w', long)]
    pub watch: bool,

    /// Path to tsconfig.json or a directory containing it.
    #[arg(short = 'p', long = "project")]
    pub project: Option<PathBuf>,

    // ==================== Language and Environment ====================
    /// Set the JavaScript language version for emitted JavaScript.
    #[arg(short = 't', long, value_enum, ignore_case = true)]
    pub target: Option<Target>,

    /// Specify what module code is generated.
    #[arg(short = 'm', long, value_enum, ignore_case = true)]
    pub module: Option<Module>,

    /// Specify a set of bundled library declaration files.
    #[arg(long, value_delimiter = ',')]
    pub lib: Option<Vec<String>>,

    /// Specify what JSX code is generated.
    #[arg(long, value_enum)]
    pub jsx: Option<JsxEmit>,

    /// Specify the JSX factory function (e.g. 'React.createElement').
    #[arg(long = "jsxFactory", alias = "jsx-factory")]
    pub jsx_factory: Option<String>,

    /// Specify the JSX Fragment reference (e.g. 'React.Fragment').
    #[arg(long = "jsxFragmentFactory", alias = "jsx-fragment-factory")]
    pub jsx_fragment_factory: Option<String>,

    /// Specify module specifier for JSX factory functions (e.g. 'react').
    #[arg(long = "jsxImportSource", alias = "jsx-import-source")]
    pub jsx_import_source: Option<String>,

    /// Disable including any library files, including the default lib.d.ts.
    #[arg(long = "noLib", alias = "no-lib")]
    pub no_lib: bool,

    /// Emit ECMAScript-standard-compliant class fields.
    #[arg(
        long = "useDefineForClassFields",
        alias = "use-define-for-class-fields"
    )]
    pub use_define_for_class_fields: Option<bool>,

    /// Control what method is used to detect module-format JS files.
    #[arg(long = "moduleDetection", alias = "module-detection", value_enum)]
    pub module_detection: Option<ModuleDetection>,

    /// Enable experimental support for legacy experimental decorators.
    #[arg(long = "experimentalDecorators", alias = "experimental-decorators")]
    pub experimental_decorators: bool,

    /// Emit design-type metadata for decorated declarations in source files.
    #[arg(long = "emitDecoratorMetadata", alias = "emit-decorator-metadata")]
    pub emit_decorator_metadata: bool,

    // ==================== Modules ====================
    /// Specify how TypeScript looks up a file from a given module specifier.
    #[arg(long = "moduleResolution", alias = "module-resolution", value_enum)]
    pub module_resolution: Option<ModuleResolution>,

    /// Specify the base directory to resolve non-relative module names.
    #[arg(long = "baseUrl", alias = "base-url")]
    pub base_url: Option<PathBuf>,

    /// Specify multiple folders that act like './node_modules/@types'.
    #[arg(long = "typeRoots", alias = "type-roots", value_delimiter = ',')]
    pub type_roots: Option<Vec<PathBuf>>,

    /// Specify type package names to be included without being referenced in a source file.
    #[arg(long, value_delimiter = ',')]
    pub types: Option<Vec<String>>,

    /// Allow multiple folders to be treated as one when resolving modules.
    #[arg(long = "rootDirs", alias = "root-dirs", value_delimiter = ',')]
    pub root_dirs: Option<Vec<PathBuf>>,

    /// Specify a set of entries that re-map imports to additional lookup locations.
    /// Accepted for CLI compatibility; normally set in tsconfig.json.
    #[arg(long, value_delimiter = ',', hide = true)]
    pub paths: Option<Vec<String>>,

    /// Specify list of language service plugins.
    /// Accepted for CLI compatibility; normally set in tsconfig.json.
    #[arg(long, value_delimiter = ',', hide = true)]
    pub plugins: Option<Vec<String>>,

    /// Enable importing .json files.
    #[arg(long = "resolveJsonModule", alias = "resolve-json-module")]
    pub resolve_json_module: bool,

    /// Use the package.json 'exports' field when resolving package imports.
    #[arg(
        long = "resolvePackageJsonExports",
        alias = "resolve-package-json-exports"
    )]
    pub resolve_package_json_exports: Option<bool>,

    /// Use the package.json 'imports' field when resolving imports.
    #[arg(
        long = "resolvePackageJsonImports",
        alias = "resolve-package-json-imports"
    )]
    pub resolve_package_json_imports: Option<bool>,

    /// List of file name suffixes to search when resolving a module.
    #[arg(
        long = "moduleSuffixes",
        alias = "module-suffixes",
        value_delimiter = ','
    )]
    pub module_suffixes: Option<Vec<String>>,

    /// Enable importing files with any extension, provided a declaration file is present.
    #[arg(
        long = "allowArbitraryExtensions",
        alias = "allow-arbitrary-extensions"
    )]
    pub allow_arbitrary_extensions: bool,

    /// Allow imports to include TypeScript file extensions.
    #[arg(
        long = "allowImportingTsExtensions",
        alias = "allow-importing-ts-extensions"
    )]
    pub allow_importing_ts_extensions: bool,

    /// Rewrite '.ts', '.tsx', '.mts', and '.cts' file extensions in relative import paths.
    #[arg(
        long = "rewriteRelativeImportExtensions",
        alias = "rewrite-relative-import-extensions"
    )]
    pub rewrite_relative_import_extensions: bool,

    /// Conditions to set in addition to the resolver-specific defaults when resolving imports.
    #[arg(
        long = "customConditions",
        alias = "custom-conditions",
        value_delimiter = ','
    )]
    pub custom_conditions: Option<Vec<String>>,

    /// Disallow 'import's, 'require's or '<reference>'s from expanding the number of files.
    #[arg(long = "noResolve", alias = "no-resolve")]
    pub no_resolve: bool,

    /// Allow accessing UMD globals from modules.
    #[arg(long = "allowUmdGlobalAccess", alias = "allow-umd-global-access")]
    pub allow_umd_global_access: bool,

    /// Check side effect imports.
    #[arg(
        long = "noUncheckedSideEffectImports",
        alias = "no-unchecked-side-effect-imports"
    )]
    pub no_unchecked_side_effect_imports: bool,

    // ==================== JavaScript Support ====================
    /// Allow JavaScript files to be a part of your program.
    #[arg(long = "allowJs", alias = "allow-js")]
    pub allow_js: bool,

    /// Enable error reporting in type-checked JavaScript files.
    #[arg(long = "checkJs", alias = "check-js")]
    pub check_js: bool,

    /// Specify the maximum folder depth used for checking JavaScript files from 'node_modules'.
    #[arg(long = "maxNodeModuleJsDepth", alias = "max-node-module-js-depth")]
    pub max_node_module_js_depth: Option<u32>,

    // ==================== Emit ====================
    /// Generate .d.ts files from TypeScript and JavaScript files in your project.
    #[arg(long = "declaration", short = 'd')]
    pub declaration: bool,

    /// Specify the output directory for generated declaration files.
    #[arg(long = "declarationDir", alias = "declaration-dir")]
    pub declaration_dir: Option<PathBuf>,

    /// Create sourcemaps for d.ts files.
    #[arg(long = "declarationMap", alias = "declaration-map")]
    pub declaration_map: bool,

    /// Only output d.ts files and not JavaScript files.
    #[arg(long = "emitDeclarationOnly", alias = "emit-declaration-only")]
    pub emit_declaration_only: bool,

    /// Create source map files for emitted JavaScript files.
    #[arg(long = "sourceMap", alias = "source-map")]
    pub source_map: bool,

    /// Include sourcemap files inside the emitted JavaScript.
    #[arg(long = "inlineSourceMap", alias = "inline-source-map")]
    pub inline_source_map: bool,

    /// Include source code in the sourcemaps inside the emitted JavaScript.
    #[arg(long = "inlineSources", alias = "inline-sources")]
    pub inline_sources: bool,

    /// Specify an output folder for all emitted files.
    #[arg(long = "outDir", alias = "out-dir")]
    pub out_dir: Option<PathBuf>,

    /// Specify the root folder within your source files.
    #[arg(long = "rootDir", alias = "root-dir")]
    pub root_dir: Option<PathBuf>,

    /// Specify a file that bundles all outputs into one JavaScript file.
    #[arg(long = "outFile", alias = "out-file")]
    pub out_file: Option<PathBuf>,

    /// Disable emitting files from a compilation.
    #[arg(long = "noEmit", alias = "no-emit")]
    pub no_emit: bool,

    /// Disable emitting files if any type checking errors are reported.
    #[arg(long = "noEmitOnError", alias = "no-emit-on-error")]
    pub no_emit_on_error: bool,

    /// Disable generating custom helper functions like '__extends' in compiled output.
    #[arg(long = "noEmitHelpers", alias = "no-emit-helpers")]
    pub no_emit_helpers: bool,

    /// Allow importing helper functions from tslib once per project.
    #[arg(long = "importHelpers", alias = "import-helpers")]
    pub import_helpers: bool,

    /// Emit more compliant, but verbose and less performant JavaScript for iteration.
    #[arg(long = "downlevelIteration", alias = "downlevel-iteration")]
    pub downlevel_iteration: bool,

    /// Specify the location where debugger should locate map files instead of generated locations.
    #[arg(long = "mapRoot", alias = "map-root")]
    pub map_root: Option<String>,

    /// Specify the root path for debuggers to find the reference source code.
    #[arg(long = "sourceRoot", alias = "source-root")]
    pub source_root: Option<String>,

    /// Set the newline character for emitting files.
    #[arg(long = "newLine", alias = "new-line", value_enum)]
    pub new_line: Option<NewLine>,

    /// Disable emitting comments.
    #[arg(long = "removeComments", alias = "remove-comments")]
    pub remove_comments: bool,

    /// Disable erasing 'const enum' declarations in generated code.
    #[arg(long = "preserveConstEnums", alias = "preserve-const-enums")]
    pub preserve_const_enums: bool,

    /// Disable emitting declarations that have '@internal' in their JSDoc comments.
    #[arg(long = "stripInternal", alias = "strip-internal")]
    pub strip_internal: bool,

    /// Emit a UTF-8 Byte Order Mark (BOM) in the beginning of output files.
    #[arg(long = "emitBOM", alias = "emit-bom")]
    pub emit_bom: bool,

    // ==================== Interop Constraints ====================
    /// Emit additional JavaScript to ease support for importing CommonJS modules.
    #[arg(long = "esModuleInterop", alias = "es-module-interop")]
    pub es_module_interop: bool,

    /// Allow 'import x from y' when a module doesn't have a default export.
    #[arg(
        long = "allowSyntheticDefaultImports",
        alias = "allow-synthetic-default-imports"
    )]
    pub allow_synthetic_default_imports: Option<bool>,

    /// Ensure that each file can be safely transpiled without relying on other imports.
    #[arg(long = "isolatedModules", alias = "isolated-modules")]
    pub isolated_modules: bool,

    /// Require sufficient annotation on exports so other tools can trivially generate declaration files.
    #[arg(long = "isolatedDeclarations", alias = "isolated-declarations")]
    pub isolated_declarations: bool,

    /// Do not transform or elide any imports or exports not marked as type-only.
    #[arg(long = "verbatimModuleSyntax", alias = "verbatim-module-syntax")]
    pub verbatim_module_syntax: bool,

    /// Ensure that casing is correct in imports.
    #[arg(
        long = "forceConsistentCasingInFileNames",
        alias = "force-consistent-casing-in-file-names"
    )]
    pub force_consistent_casing_in_file_names: Option<bool>,

    /// Disable resolving symlinks to their realpath.
    #[arg(long = "preserveSymlinks", alias = "preserve-symlinks")]
    pub preserve_symlinks: bool,

    /// Do not allow runtime constructs that are not part of ECMAScript.
    #[arg(long = "erasableSyntaxOnly", alias = "erasable-syntax-only")]
    pub erasable_syntax_only: bool,

    // ==================== Type Checking ====================
    /// Enable all strict type-checking options.
    #[arg(long)]
    pub strict: bool,

    /// Enable error reporting for expressions and declarations with an implied 'any' type.
    #[arg(long = "noImplicitAny", alias = "no-implicit-any")]
    pub no_implicit_any: Option<bool>,

    /// When type checking, take into account 'null' and 'undefined'.
    #[arg(long = "strictNullChecks", alias = "strict-null-checks")]
    pub strict_null_checks: Option<bool>,

    /// When assigning functions, check to ensure parameters and the return values are subtype-compatible.
    #[arg(long = "strictFunctionTypes", alias = "strict-function-types")]
    pub strict_function_types: Option<bool>,

    /// Check that the arguments for 'bind', 'call', and 'apply' methods match the original function.
    #[arg(long = "strictBindCallApply", alias = "strict-bind-call-apply")]
    pub strict_bind_call_apply: Option<bool>,

    /// Check for class properties that are declared but not set in the constructor.
    #[arg(
        long = "strictPropertyInitialization",
        alias = "strict-property-initialization"
    )]
    pub strict_property_initialization: Option<bool>,

    /// Built-in iterators are instantiated with a 'TReturn' type of 'undefined' instead of 'any'.
    #[arg(
        long = "strictBuiltinIteratorReturn",
        alias = "strict-builtin-iterator-return"
    )]
    pub strict_builtin_iterator_return: Option<bool>,

    /// Enable error reporting when 'this' is given the type 'any'.
    #[arg(long = "noImplicitThis", alias = "no-implicit-this")]
    pub no_implicit_this: Option<bool>,

    /// Default catch clause variables as 'unknown' instead of 'any'.
    #[arg(
        long = "useUnknownInCatchVariables",
        alias = "use-unknown-in-catch-variables"
    )]
    pub use_unknown_in_catch_variables: Option<bool>,

    /// Ensure 'use strict' is always emitted.
    #[arg(long = "alwaysStrict", alias = "always-strict")]
    pub always_strict: Option<bool>,

    /// Enable error reporting when local variables aren't read.
    #[arg(long = "noUnusedLocals", alias = "no-unused-locals")]
    pub no_unused_locals: bool,

    /// Raise an error when a function parameter isn't read.
    #[arg(long = "noUnusedParameters", alias = "no-unused-parameters")]
    pub no_unused_parameters: bool,

    /// Interpret optional property types as written, rather than adding 'undefined'.
    #[arg(
        long = "exactOptionalPropertyTypes",
        alias = "exact-optional-property-types"
    )]
    pub exact_optional_property_types: bool,

    /// Enable error reporting for codepaths that do not explicitly return in a function.
    #[arg(long = "noImplicitReturns", alias = "no-implicit-returns")]
    pub no_implicit_returns: bool,

    /// Enable error reporting for fallthrough cases in switch statements.
    #[arg(
        long = "noFallthroughCasesInSwitch",
        alias = "no-fallthrough-cases-in-switch"
    )]
    pub no_fallthrough_cases_in_switch: bool,

    /// Enable Sound Mode for stricter type checking beyond TypeScript's defaults.
    /// Catches common unsoundness like mutable array covariance, method bivariance,
    /// `any` escapes, and excess properties via sticky freshness.
    /// Uses TS9xxx diagnostic codes (TS9001-TS9008).
    #[arg(long)]
    pub sound: bool,

    /// Add 'undefined' to a type when accessed using an index.
    #[arg(
        long = "noUncheckedIndexedAccess",
        alias = "no-unchecked-indexed-access"
    )]
    pub no_unchecked_indexed_access: bool,

    /// Ensure overriding members in derived classes are marked with an override modifier.
    #[arg(long = "noImplicitOverride", alias = "no-implicit-override")]
    pub no_implicit_override: bool,

    /// Enforces using indexed accessors for keys declared using an indexed type.
    #[arg(
        long = "noPropertyAccessFromIndexSignature",
        alias = "no-property-access-from-index-signature"
    )]
    pub no_property_access_from_index_signature: bool,

    /// Disable error reporting for unreachable code.
    #[arg(long = "allowUnreachableCode", alias = "allow-unreachable-code")]
    pub allow_unreachable_code: Option<bool>,

    /// Disable error reporting for unused labels.
    #[arg(long = "allowUnusedLabels", alias = "allow-unused-labels")]
    pub allow_unused_labels: Option<bool>,

    // ==================== Completeness ====================
    /// Skip type checking .d.ts files that are included with TypeScript.
    #[arg(long = "skipDefaultLibCheck", alias = "skip-default-lib-check")]
    pub skip_default_lib_check: bool,

    /// Skip type checking all .d.ts files.
    #[arg(long = "skipLibCheck", alias = "skip-lib-check")]
    pub skip_lib_check: bool,

    // ==================== Projects ====================
    /// Enable constraints that allow a TypeScript project to be used with project references.
    #[arg(long)]
    pub composite: bool,

    /// Save .tsbuildinfo files to allow for incremental compilation of projects.
    #[arg(short = 'i', long)]
    pub incremental: bool,

    /// Specify the path to .tsbuildinfo incremental compilation file.
    #[arg(long = "tsBuildInfoFile", alias = "ts-build-info-file")]
    pub ts_build_info_file: Option<PathBuf>,

    /// Reduce the number of projects loaded automatically by TypeScript.
    #[arg(
        long = "disableReferencedProjectLoad",
        alias = "disable-referenced-project-load"
    )]
    pub disable_referenced_project_load: bool,

    /// Opt a project out of multi-project reference checking when editing.
    #[arg(
        long = "disableSolutionSearching",
        alias = "disable-solution-searching"
    )]
    pub disable_solution_searching: bool,

    /// Disable preferring source files instead of declaration files when referencing composite projects.
    #[arg(
        long = "disableSourceOfProjectReferenceRedirect",
        alias = "disable-source-of-project-reference-redirect"
    )]
    pub disable_source_of_project_reference_redirect: bool,

    // ==================== Compiler Diagnostics ====================
    /// Output compiler performance information after building.
    #[arg(long)]
    pub diagnostics: bool,

    /// Output more detailed compiler performance information after building.
    #[arg(long = "extendedDiagnostics", alias = "extended-diagnostics")]
    pub extended_diagnostics: bool,

    /// Print files read during the compilation including why it was included.
    #[arg(long = "explainFiles", alias = "explain-files")]
    pub explain_files: bool,

    /// Print all of the files read during the compilation.
    #[arg(long = "listFiles", alias = "list-files")]
    pub list_files: bool,

    /// Print the names of emitted files after a compilation.
    #[arg(long = "listEmittedFiles", alias = "list-emitted-files")]
    pub list_emitted_files: bool,

    /// Log paths used during the 'moduleResolution' process.
    #[arg(long = "traceResolution", alias = "trace-resolution")]
    pub trace_resolution: bool,

    /// Log all dependencies that were resolved during compilation.
    #[arg(long = "traceDependencies", alias = "trace-dependencies")]
    pub trace_dependencies: bool,

    /// Generates an event trace and a list of types.
    #[arg(long = "generateTrace", alias = "generate-trace")]
    pub generate_trace: Option<PathBuf>,

    /// Emit a v8 CPU profile of the compiler run for debugging.
    #[arg(long = "generateCpuProfile", alias = "generate-cpu-profile")]
    pub generate_cpu_profile: Option<PathBuf>,

    /// Disable full type checking (only critical parse and emit errors will be reported).
    #[arg(long = "noCheck", alias = "no-check")]
    pub no_check: bool,

    // ==================== Output Formatting ====================
    /// Enable color and formatting in TypeScript's output to make compiler errors easier to read.
    #[arg(long)]
    pub pretty: Option<bool>,

    /// Disable truncating types in error messages.
    #[arg(long = "noErrorTruncation", alias = "no-error-truncation")]
    pub no_error_truncation: bool,

    /// Disable wiping the console in watch mode.
    #[arg(long = "preserveWatchOutput", alias = "preserve-watch-output")]
    pub preserve_watch_output: bool,

    // ==================== Watch Mode Options ====================
    /// Specify how the TypeScript watch mode works.
    #[arg(long = "watchFile", alias = "watch-file", value_enum)]
    pub watch_file: Option<WatchFileKind>,

    /// Specify how directories are watched on systems that lack recursive file-watching functionality.
    #[arg(long = "watchDirectory", alias = "watch-directory", value_enum)]
    pub watch_directory: Option<WatchDirectoryKind>,

    /// Specify what approach the watcher should use if the system runs out of native file watchers.
    #[arg(long = "fallbackPolling", alias = "fallback-polling", value_enum)]
    pub fallback_polling: Option<PollingWatchKind>,

    /// Synchronously call callbacks and update the state of directory watchers.
    #[arg(
        long = "synchronousWatchDirectory",
        alias = "synchronous-watch-directory"
    )]
    pub synchronous_watch_directory: bool,

    /// Remove a list of directories from the watch process.
    #[arg(
        long = "excludeDirectories",
        alias = "exclude-directories",
        value_delimiter = ','
    )]
    pub exclude_directories: Option<Vec<PathBuf>>,

    /// Remove a list of files from the watch mode's processing.
    #[arg(long = "excludeFiles", alias = "exclude-files", value_delimiter = ',')]
    pub exclude_files: Option<Vec<PathBuf>>,

    // ==================== Build Mode Options ====================
    /// Enable verbose logging (build mode).
    #[arg(long, visible_alias = "verbose")]
    pub build_verbose: bool,

    /// Show what would be built (or deleted, if specified with '--clean').
    #[arg(long)]
    pub dry: bool,

    /// Build all projects, including those that appear to be up to date.
    #[arg(short = 'f', long)]
    pub force: bool,

    /// Delete the outputs of all projects.
    #[arg(long)]
    pub clean: bool,

    /// Skip building downstream projects on error in upstream project.
    #[arg(long = "stopBuildOnErrors", alias = "stop-build-on-errors")]
    pub stop_build_on_errors: bool,

    // ==================== Watch and Build Modes ====================
    /// Have recompiles in projects that use 'incremental' and 'watch' mode assume changes only affect direct dependencies.
    #[arg(
        long = "assumeChangesOnlyAffectDirectDependencies",
        alias = "assume-changes-only-affect-direct-dependencies"
    )]
    pub assume_changes_only_affect_direct_dependencies: bool,

    // ==================== Backwards Compatibility ====================
    /// Deprecated: Specify the object invoked for 'createElement' (use --jsxFactory instead).
    #[arg(long = "reactNamespace", alias = "react-namespace", hide = true)]
    pub react_namespace: Option<String>,

    /// Deprecated: In early versions, manually set the text encoding for reading files.
    #[arg(long, hide = true)]
    pub charset: Option<String>,

    /// Deprecated: Specify emit/checking behavior for imports that are only used for types.
    #[arg(
        long = "importsNotUsedAsValues",
        alias = "imports-not-used-as-values",
        value_enum,
        hide = true
    )]
    pub imports_not_used_as_values: Option<ImportsNotUsedAsValues>,

    /// Deprecated: Make keyof only return strings instead of string, numbers or symbols.
    #[arg(long = "keyofStringsOnly", alias = "keyof-strings-only", hide = true)]
    pub keyof_strings_only: bool,

    /// Deprecated: Disable adding 'use strict' directives in emitted JavaScript files.
    #[arg(
        long = "noImplicitUseStrict",
        alias = "no-implicit-use-strict",
        hide = true
    )]
    pub no_implicit_use_strict: bool,

    /// Deprecated: Disable strict checking of generic signatures in function types.
    #[arg(
        long = "noStrictGenericChecks",
        alias = "no-strict-generic-checks",
        hide = true
    )]
    pub no_strict_generic_checks: bool,

    /// Deprecated: Use 'outFile' instead.
    #[arg(long, hide = true)]
    pub out: Option<PathBuf>,

    /// Deprecated: Preserve unused imported values in the JavaScript output.
    #[arg(
        long = "preserveValueImports",
        alias = "preserve-value-imports",
        hide = true
    )]
    pub preserve_value_imports: bool,

    /// Deprecated: Disable reporting of excess property errors during the creation of object literals.
    #[arg(
        long = "suppressExcessPropertyErrors",
        alias = "suppress-excess-property-errors",
        hide = true
    )]
    pub suppress_excess_property_errors: bool,

    /// Deprecated: Suppress 'noImplicitAny' errors when indexing objects that lack index signatures.
    #[arg(
        long = "suppressImplicitAnyIndexErrors",
        alias = "suppress-implicit-any-index-errors",
        hide = true
    )]
    pub suppress_implicit_any_index_errors: bool,

    // ==================== Editor Support ====================
    /// Remove the 20mb cap on total source code size for JavaScript files in the TypeScript language server.
    #[arg(long = "disableSizeLimit", alias = "disable-size-limit")]
    pub disable_size_limit: bool,

    // ==================== Custom Options ====================
    /// Override the compiler version used for typesVersions resolution
    /// (or set TSZ_TYPES_VERSIONS_COMPILER_VERSION).
    #[arg(
        long = "typesVersions",
        alias = "types-versions",
        value_name = "VERSION"
    )]
    pub types_versions_compiler_version: Option<String>,

    // ==================== Input Files ====================
    /// Input files to compile.
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Target {
    Es3,
    Es5,
    #[value(alias = "es6")]
    Es2015,
    Es2016,
    Es2017,
    Es2018,
    Es2019,
    Es2020,
    Es2021,
    Es2022,
    Es2023,
    Es2024,
    #[value(name = "esnext", alias = "es-next")]
    EsNext,
}

impl Target {
    pub fn to_script_target(self) -> ScriptTarget {
        match self {
            Target::Es3 => ScriptTarget::ES3,
            Target::Es5 => ScriptTarget::ES5,
            Target::Es2015 => ScriptTarget::ES2015,
            Target::Es2016 => ScriptTarget::ES2016,
            Target::Es2017 => ScriptTarget::ES2017,
            Target::Es2018 => ScriptTarget::ES2018,
            Target::Es2019 => ScriptTarget::ES2019,
            Target::Es2020 => ScriptTarget::ES2020,
            Target::Es2021 => ScriptTarget::ES2021,
            Target::Es2022 => ScriptTarget::ES2022,
            Target::Es2023 => ScriptTarget::ES2022, // Map to ES2022 until ES2023 support is added
            Target::Es2024 => ScriptTarget::ES2022, // Map to ES2022 until ES2024 support is added
            Target::EsNext => ScriptTarget::ESNext,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Module {
    None,
    #[value(name = "commonjs", alias = "common-js")]
    CommonJs,
    Amd,
    Umd,
    System,
    #[value(alias = "es6")]
    Es2015,
    Es2020,
    Es2022,
    #[value(name = "esnext", alias = "es-next")]
    EsNext,
    #[value(name = "node16", alias = "node-16")]
    Node16,
    #[value(name = "node18", alias = "node-18")]
    Node18,
    #[value(name = "node20", alias = "node-20")]
    Node20,
    #[value(name = "nodenext", alias = "node-next")]
    NodeNext,
    /// Preserve the original module syntax.
    Preserve,
}

impl Module {
    pub fn to_module_kind(self) -> ModuleKind {
        match self {
            Module::None => ModuleKind::None,
            Module::CommonJs => ModuleKind::CommonJS,
            Module::Amd => ModuleKind::AMD,
            Module::Umd => ModuleKind::UMD,
            Module::System => ModuleKind::System,
            Module::Es2015 => ModuleKind::ES2015,
            Module::Es2020 => ModuleKind::ES2020,
            Module::Es2022 => ModuleKind::ES2022,
            Module::EsNext => ModuleKind::ESNext,
            Module::Node16 => ModuleKind::Node16,
            Module::Node18 => ModuleKind::Node16, // Map to Node16 until separate support
            Module::Node20 => ModuleKind::Node16, // Map to Node16 until separate support
            Module::NodeNext => ModuleKind::NodeNext,
            Module::Preserve => ModuleKind::ESNext, // Map to ESNext for preserve mode
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum JsxEmit {
    /// Keep the JSX as part of the output to be further transformed by another transform step.
    Preserve,
    /// Emit .js files with JSX changed to the equivalent React.createElement calls.
    React,
    /// Emit .js files with the JSX changed to _jsx calls.
    #[value(name = "react-jsx")]
    ReactJsx,
    /// Emit .js files with the JSX changed to _jsx calls (development mode).
    #[value(name = "react-jsxdev")]
    ReactJsxDev,
    /// Keep the JSX as part of the output (like preserve), but also emit .js files.
    #[value(name = "react-native")]
    ReactNative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ModuleResolution {
    /// Deprecated: TypeScript 1.6 resolution strategy.
    Classic,
    /// Node.js style resolution for CommonJS.
    #[value(alias = "node")]
    Node10,
    /// Node.js 16+ resolution for ES modules and CommonJS.
    Node16,
    /// Latest Node.js resolution for ES modules and CommonJS.
    #[value(name = "nodenext", alias = "node-next")]
    NodeNext,
    /// Resolution for bundlers (Webpack, Rollup, esbuild, etc).
    Bundler,
}

impl ModuleResolution {
    pub fn to_module_resolution_kind(self) -> ModuleResolutionKind {
        match self {
            ModuleResolution::Classic => ModuleResolutionKind::Classic,
            ModuleResolution::Node10 => ModuleResolutionKind::Node,
            ModuleResolution::Node16 => ModuleResolutionKind::Node16,
            ModuleResolution::NodeNext => ModuleResolutionKind::NodeNext,
            ModuleResolution::Bundler => ModuleResolutionKind::Bundler,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ModuleDetection {
    /// Files with imports, exports, import.meta, jsx, or esm format are modules.
    Auto,
    /// Every non-declaration file is a module.
    Force,
    /// Only files with imports or exports are modules.
    Legacy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum NewLine {
    /// Use carriage return followed by line feed (\\r\\n).
    Crlf,
    /// Use line feed only (\\n).
    Lf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum WatchFileKind {
    /// Poll files at fixed intervals.
    #[value(name = "fixedpollinginterval", alias = "fixed-polling-interval")]
    FixedPollingInterval,
    /// Poll files with priority intervals.
    #[value(name = "prioritypollinginterval", alias = "priority-polling-interval")]
    PriorityPollingInterval,
    /// Poll files dynamically based on activity.
    #[value(name = "dynamicprioritypolling", alias = "dynamic-priority-polling")]
    DynamicPriorityPolling,
    /// Poll using fixed chunk sizes.
    #[value(name = "fixedchunksizepolling", alias = "fixed-chunk-size-polling")]
    FixedChunkSizePolling,
    /// Use native file system events.
    #[value(name = "usefsevents", alias = "use-fs-events")]
    UseFsEvents,
    /// Use file system events on parent directory.
    #[value(
        name = "usefseventsonparentdirectory",
        alias = "use-fs-events-on-parent-directory"
    )]
    UseFsEventsOnParentDirectory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum WatchDirectoryKind {
    /// Use native file system events for directories.
    #[value(name = "usefsevents", alias = "use-fs-events")]
    UseFsEvents,
    /// Poll directories at fixed intervals.
    #[value(name = "fixedpollinginterval", alias = "fixed-polling-interval")]
    FixedPollingInterval,
    /// Poll directories dynamically based on activity.
    #[value(name = "dynamicprioritypolling", alias = "dynamic-priority-polling")]
    DynamicPriorityPolling,
    /// Poll directories using fixed chunk sizes.
    #[value(name = "fixedchunksizepolling", alias = "fixed-chunk-size-polling")]
    FixedChunkSizePolling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum PollingWatchKind {
    /// Poll at fixed intervals as fallback.
    #[value(name = "fixedinterval", alias = "fixed-interval")]
    FixedInterval,
    /// Poll with priority intervals as fallback.
    #[value(name = "priorityinterval", alias = "priority-interval")]
    PriorityInterval,
    /// Poll dynamically as fallback.
    #[value(name = "dynamicpriority", alias = "dynamic-priority")]
    DynamicPriority,
    /// Poll using fixed chunk sizes as fallback.
    #[value(name = "fixedchunksize", alias = "fixed-chunk-size")]
    FixedChunkSize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ImportsNotUsedAsValues {
    /// Drop import statements which only reference types.
    Remove,
    /// Preserve all import statements.
    Preserve,
    /// Error on import statements that only reference types.
    Error,
}
