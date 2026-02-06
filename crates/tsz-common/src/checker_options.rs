//! Compiler options for type checking.
//!
//! This module lives in tsz-common so that both the solver and checker
//! can reference `CheckerOptions` without creating a circular dependency.

use crate::common::{ModuleKind, ScriptTarget};

/// Compiler options for type checking.
#[derive(Debug, Clone, Default)]
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
    /// Module kind (None, CommonJS, ES2015, ES2020, ES2022, ESNext, etc.)
    /// Controls which module system is being targeted (affects import/export syntax validity)
    pub module: ModuleKind,
    /// Emit additional JavaScript to ease support for importing CommonJS modules.
    /// When true, synthesizes default exports for CommonJS modules.
    pub es_module_interop: bool,
    /// Allow 'import x from y' when a module doesn't have a default export.
    /// Implied by esModuleInterop.
    pub allow_synthetic_default_imports: bool,
    /// When true, disable error reporting for unreachable code (TS7027).
    pub allow_unreachable_code: bool,
    /// When true, require bracket notation for index signature property access (TS4111).
    pub no_property_access_from_index_signature: bool,
    /// When true, enable Sound Mode for stricter type checking beyond TypeScript's defaults.
    /// Sound Mode catches common unsoundness issues like:
    /// - Mutable array covariance (TS9002)
    /// - Method parameter bivariance (TS9003)
    /// - `any` escapes (TS9004)
    /// - Excess properties via sticky freshness (TS9001)
    ///
    /// Activated via: `--sound` CLI flag or `// @ts-sound` pragma
    pub sound_mode: bool,
    /// When true, enables experimental support for decorators (legacy decorators).
    /// This is required for the @experimentalDecorators flag.
    /// When decorators are used, TypedPropertyDescriptor must be available.
    pub experimental_decorators: bool,
    /// When true, report errors for unused local variables (TS6133).
    pub no_unused_locals: bool,
    /// When true, report errors for unused function parameters (TS6133).
    pub no_unused_parameters: bool,
}

impl CheckerOptions {
    /// Apply TypeScript's `--strict` defaults to individual strict flags.
    /// In tsc, enabling `strict` turns on the strict family unless explicitly disabled.
    /// We mirror that behavior by OR-ing the per-flag booleans with `strict`.
    pub fn apply_strict_defaults(mut self) -> Self {
        if self.strict {
            self.no_implicit_any = true;
            self.no_implicit_this = true;
            self.strict_null_checks = true;
            self.strict_function_types = true;
            self.strict_bind_call_apply = true;
            self.strict_property_initialization = true;
            self.use_unknown_in_catch_variables = true;
            // exactOptionalPropertyTypes and other opts are not implied by --strict
        }
        self
    }
}
