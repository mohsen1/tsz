//! Compiler options for type checking.
//!
//! This module lives in tsz-common so that both the solver and checker
//! can reference `CheckerOptions` without creating a circular dependency.

use crate::common::{ModuleKind, ScriptTarget};

/// Compiler options for type checking.
#[derive(Debug, Clone)]
pub struct CheckerOptions {
    pub strict: bool,
    pub no_implicit_any: bool,
    pub no_implicit_returns: bool,
    pub strict_null_checks: bool,
    pub strict_function_types: bool,
    pub strict_property_initialization: bool,
    pub no_implicit_this: bool,
    pub use_unknown_in_catch_variables: bool,
    pub isolated_modules: bool,
    /// When true, indexed access with index signatures adds `| undefined` to the type
    pub no_unchecked_indexed_access: bool,
    /// When true, checking bind/call/apply uses strict function signatures
    pub strict_bind_call_apply: bool,
    /// When true, optional properties are treated as exactly `T | undefined` not `T | undefined | missing`
    pub exact_optional_property_types: bool,
    /// When true, no library files (including lib.d.ts) are included.
    /// This corresponds to the --noLib compiler flag.
    /// TS2318 errors are emitted when referencing global types with this option enabled.
    pub no_lib: bool,
    /// When true, do not automatically inject built-in type declarations.
    /// This corresponds to the --noTypesAndSymbols compiler flag.
    /// Prevents loading default lib.d.ts files which provide types like Array, Object, etc.
    pub no_types_and_symbols: bool,
    /// Target ECMAScript version (ES3, ES5, ES2015, ES2016, etc.)
    /// Controls which built-in types are available (e.g., Promise requires ES2015)
    /// Defaults to ES3 for maximum compatibility
    pub target: ScriptTarget,
    /// Module kind (None, `CommonJS`, ES2015, ES2020, ES2022, `ESNext`, etc.)
    /// Controls which module system is being targeted (affects import/export syntax validity)
    pub module: ModuleKind,
    /// Emit additional JavaScript to ease support for importing `CommonJS` modules.
    /// When true, synthesizes default exports for `CommonJS` modules.
    pub es_module_interop: bool,
    /// Allow 'import x from y' when a module doesn't have a default export.
    /// Implied by esModuleInterop.
    pub allow_synthetic_default_imports: bool,
    /// Controls reporting of unreachable code (TS7027).
    /// - `None` (default): tsc emits TS7027 as a suggestion, not an error
    /// - `Some(false)`: tsc emits TS7027 as an error
    /// - `Some(true)`: tsc does not emit TS7027 at all
    pub allow_unreachable_code: Option<bool>,
    /// Controls reporting of unused labels (TS7028).
    /// - `None` (default): tsc emits TS7028 as a suggestion, not an error
    /// - `Some(false)`: tsc emits TS7028 as an error
    /// - `Some(true)`: tsc does not emit TS7028 at all
    pub allow_unused_labels: Option<bool>,
    /// When true, require bracket notation for index signature property access (TS4111).
    pub no_property_access_from_index_signature: bool,
    /// When true, enable Sound Mode for stricter type checking beyond TypeScript's defaults.
    /// Sound Mode catches common unsoundness issues like:
    /// - Mutable array covariance (TS9002)
    /// - Method parameter bivariance (TS9003)
    /// - `any` escapes (TS9004)
    /// - Excess properties via sticky freshness (TS9001)
    ///
    /// Activated via: `--sound` CLI flag or `// @tsz-sound` pragma
    pub sound_mode: bool,
    /// When true, enables experimental support for decorators (legacy decorators).
    /// This is required for the @experimentalDecorators flag.
    /// When decorators are used, `TypedPropertyDescriptor` must be available.
    pub experimental_decorators: bool,
    /// When true, report errors for unused local variables (TS6133).
    pub no_unused_locals: bool,
    /// When true, report errors for unused function parameters (TS6133).
    pub no_unused_parameters: bool,
    /// When true, parse in strict mode and emit "use strict" for each source file.
    pub always_strict: bool,
    /// When true, do not add "use strict" to emitted output even with `alwaysStrict`.
    /// Also suppresses strict-mode checking rules that stem solely from `alwaysStrict`
    /// (e.g. TS1100 for `var arguments`), matching tsc behaviour.
    pub no_implicit_use_strict: bool,
    /// When true, allows importing JSON files with `.json` extension.
    /// When false, importing JSON files emits TS2732 suggesting to enable this flag.
    pub resolve_json_module: bool,
    /// When true, enable type checking in JavaScript files.
    /// This corresponds to the --checkJs compiler flag.
    /// With checkJs enabled, noImplicitAny and other type errors apply to .js files.
    pub check_js: bool,
    /// When true, allow JavaScript files to participate in module/reference resolution.
    /// This corresponds to the --allowJs compiler flag.
    pub allow_js: bool,
    /// When true, disable dependency expansion from imports and triple-slash references.
    pub no_resolve: bool,
    /// When true, declaration emit must be computable per-file without cross-file inference.
    /// This corresponds to the --isolatedDeclarations compiler flag.
    pub isolated_declarations: bool,
    /// When true, declarations will be emitted for this program.
    /// Used by checker diagnostics that only matter when `.d.ts` serialization is required.
    pub emit_declarations: bool,
    /// When true, check side-effect imports for module resolution errors (TS2882).
    pub no_unchecked_side_effect_imports: bool,
    /// When true, require 'override' modifier on members that override base class members (TS4114).
    pub no_implicit_override: bool,
    /// JSX factory function (e.g. `React.createElement`)
    pub jsx_factory: String,
    /// Whether `jsxFactory` was explicitly set via compiler options.
    /// When true, tsc 6.0 skips the factory-in-scope check (no TS2874).
    /// When false, the factory name comes from `reactNamespace` or the default `React.createElement`,
    /// and tsc 6.0 checks scope and emits TS2874 if not found.
    pub jsx_factory_from_config: bool,
    /// JSX fragment factory function (e.g. `React.Fragment`)
    pub jsx_fragment_factory: String,
    /// Whether `jsxFragmentFactory` was explicitly set via compiler options.
    pub jsx_fragment_factory_from_config: bool,
    /// JSX emit mode (preserve, react, react-jsx, react-jsxdev, react-native).
    /// Only "react" (classic transform) requires the factory to be in scope.
    pub jsx_mode: JsxMode,
    /// Whether the `module` option was explicitly set by the user (via tsconfig or CLI).
    /// When false, the module kind was computed from defaults (e.g. based on target).
    /// tsc only emits TS1202 (import assignment in ESM) when module is explicitly set.
    pub module_explicitly_set: bool,
    /// When true, suppress TS2353 (excess property) errors.
    /// This is a removed option (TS5102) but tsc still honors its suppression behavior.
    pub suppress_excess_property_errors: bool,
    /// When true, suppress TS7053 (implicit any index) errors.
    /// This is a removed option (TS5102) but tsc still honors its suppression behavior.
    pub suppress_implicit_any_index_errors: bool,
    /// When true, allow import paths to end with `.ts`, `.tsx`, `.mts`, `.cts` extensions.
    /// When false (default), such imports emit TS5097.
    pub allow_importing_ts_extensions: bool,
    /// When true, `.ts` extensions in relative imports are rewritten to `.js` in output.
    /// Implies the same TS5097 suppression as `allow_importing_ts_extensions`.
    pub rewrite_relative_import_extensions: bool,
    /// When true, the effective module resolution is Classic.
    /// Used by `module_not_found_diagnostic()` to decide between TS2792 and TS2307.
    /// TS2792 ("Did you mean to set moduleResolution to nodenext?") is only emitted
    /// when Classic resolution is in effect.
    pub implied_classic_resolution: bool,
    /// JSX import source for automatic JSX transform (react-jsx/react-jsxdev).
    /// When set (e.g., "react", "preact"), the compiler checks that
    /// `<source>/jsx-runtime` can be resolved. If not, TS2875 is emitted.
    /// Empty string means not set (default).
    pub jsx_import_source: String,
    /// When true, do not transform or elide any imports or exports not marked as type-only.
    /// Under this mode, importing a `.d.ts` file without `import type` is an error (TS2846).
    pub verbatim_module_syntax: bool,
    /// When true, suppress deprecation warnings (e.g., TS2880 for `assert` import assertions).
    /// Set when `ignoreDeprecations` is "5.0" or "6.0".
    pub ignore_deprecations: bool,
    /// When true, allow accessing UMD globals from modules without importing them.
    /// Suppresses TS2686 ("refers to a UMD global, but the current file is a module").
    pub allow_umd_global_access: bool,
    /// When true, keep const enum declarations in emitted code.
    /// When false (default), const enums are erased and don't affect control flow.
    pub preserve_const_enums: bool,
    /// When true, built-in iterators (Array, Map, Set, etc.) have `BuiltinIteratorReturn`
    /// resolved to `undefined` instead of `any`. Implied by `--strict` (TS 5.6+).
    pub strict_builtin_iterator_return: bool,
    /// When true, only allow syntax that can be fully erased (no runtime emit).
    /// Disallows parameter properties, enums, namespaces, import=, export=, and
    /// angle-bracket type assertions. Reports TS1294.
    pub erasable_syntax_only: bool,
}

/// JSX emit mode controlling how JSX is transformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsxMode {
    /// No JSX mode specified (default — treated same as None/no JSX).
    #[default]
    None,
    /// Keep JSX as-is in the output (no factory required in scope).
    Preserve,
    /// Classic React transform — requires factory (e.g. `React.createElement`) in scope.
    React,
    /// Automatic React transform via `_jsx` — factory NOT required in scope.
    ReactJsx,
    /// Development automatic React transform — factory NOT required in scope.
    ReactJsxDev,
    /// React Native — preserve JSX (no factory required in scope).
    ReactNative,
}

#[cfg(test)]
#[path = "../tests/checker_options_tests.rs"]
mod tests;

impl Default for CheckerOptions {
    fn default() -> Self {
        Self {
            strict: true,
            no_implicit_any: true,
            no_implicit_returns: false,
            strict_null_checks: true,
            strict_function_types: true,
            strict_property_initialization: true,
            no_implicit_this: true,
            use_unknown_in_catch_variables: true,
            isolated_modules: false,
            no_unchecked_indexed_access: false,
            strict_bind_call_apply: true,
            exact_optional_property_types: false,
            no_lib: false,
            no_types_and_symbols: false,
            target: ScriptTarget::default(),
            module: ModuleKind::default(),
            es_module_interop: false,
            allow_synthetic_default_imports: false,
            allow_unreachable_code: None,
            allow_unused_labels: None,
            no_property_access_from_index_signature: false,
            sound_mode: false,
            experimental_decorators: false,
            no_unused_locals: false,
            no_unused_parameters: false,
            // TSC 6.0 defaults: `alwaysStrict !== false` → true when not explicitly set.
            // This matches TypeScript's behavior where alwaysStrict is true by default.
            always_strict: true,
            no_implicit_use_strict: false,
            resolve_json_module: false,
            check_js: false,
            allow_js: false,
            no_resolve: false,
            isolated_declarations: false,
            emit_declarations: false,
            no_unchecked_side_effect_imports: true,
            no_implicit_override: false,
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            jsx_mode: JsxMode::None,
            module_explicitly_set: false,
            suppress_excess_property_errors: false,
            suppress_implicit_any_index_errors: false,
            allow_importing_ts_extensions: false,
            rewrite_relative_import_extensions: false,
            implied_classic_resolution: false,
            jsx_import_source: String::new(),
            verbatim_module_syntax: false,
            ignore_deprecations: false,
            allow_umd_global_access: false,
            preserve_const_enums: false,
            strict_builtin_iterator_return: true,
            erasable_syntax_only: false,
        }
    }
}

impl CheckerOptions {
    /// Apply TypeScript's `--strict` defaults to individual strict flags.
    /// In tsc, enabling `strict` turns on the strict family unless explicitly disabled.
    /// We mirror that behavior by OR-ing the per-flag booleans with `strict`.
    #[must_use]
    pub const fn apply_strict_defaults(mut self) -> Self {
        if self.strict {
            self.no_implicit_any = true;
            self.no_implicit_this = true;
            self.strict_null_checks = true;
            self.strict_function_types = true;
            self.strict_bind_call_apply = true;
            self.strict_property_initialization = true;
            self.use_unknown_in_catch_variables = true;
            self.always_strict = true;
            self.strict_builtin_iterator_return = true;
            // exactOptionalPropertyTypes and other opts are not implied by --strict
        }
        self
    }
}
