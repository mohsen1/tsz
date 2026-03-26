//! Compiler option accessors for `CheckerContext`.
//!
//! These methods provide convenient access to the `CheckerOptions` flags
//! and derive solver configuration (`JudgeConfig`, `CompatChecker`) from them.

use tsz_parser::parser::NodeIndex;
use tsz_solver::judge::JudgeConfig;

use super::CheckerContext;

/// Check if a file name represents a declaration file (.d.ts, .d.tsx, .d.mts, .d.cts).
///
/// Use this for checking file names other than the current file.
/// For the current file, prefer `CheckerContext::is_declaration_file()`.
pub(crate) fn is_declaration_file_name(file_name: &str) -> bool {
    file_name.ends_with(".d.ts")
        || file_name.ends_with(".d.tsx")
        || file_name.ends_with(".d.mts")
        || file_name.ends_with(".d.cts")
}

impl<'a> CheckerContext<'a> {
    // =========================================================================
    // Compiler Option Accessors
    // =========================================================================

    /// Check if strict mode is enabled.
    pub const fn is_strict_mode(&self) -> bool {
        self.compiler_options.strict
    }

    /// Check if the current file is a declaration file (.d.ts, .d.tsx, .d.mts, .d.cts).
    pub fn is_declaration_file(&self) -> bool {
        is_declaration_file_name(&self.file_name)
    }

    /// Check if a declaration is ambient (in a `.d.ts` file, has `declare` keyword,
    /// AMBIENT node flag, or is inside an ambient context like `declare module`).
    pub fn is_ambient_declaration(&self, idx: NodeIndex) -> bool {
        self.is_declaration_file() || self.arena.is_in_ambient_context(idx)
    }

    /// Check if the current file is a JavaScript file (.js, .jsx, .mjs, .cjs).
    pub fn is_js_file(&self) -> bool {
        self.file_name.ends_with(".js")
            || self.file_name.ends_with(".jsx")
            || self.file_name.ends_with(".mjs")
            || self.file_name.ends_with(".cjs")
    }

    /// Check whether JS strict-mode diagnostics should be enforced for the current file.
    ///
    /// In the conformance harness, `@strict: false` suppresses `alwaysStrict`-driven JS
    /// strict-mode diagnostics unless `@alwaysStrict` explicitly opts back in.
    pub fn js_strict_mode_diagnostics_enabled(&self) -> bool {
        !self.is_js_file()
            || (self.compiler_options.always_strict
                && !self.compiler_options.no_implicit_use_strict)
    }

    /// Check if JSDoc type annotations should be resolved for the current file.
    /// Returns `true` for TypeScript files (always) and for JS files when either
    /// the global `--checkJs` flag is set or the file contains a `// @ts-check` pragma.
    pub fn should_resolve_jsdoc(&self) -> bool {
        if !self.is_js_file() {
            return true;
        }
        if self.compiler_options.check_js {
            return true;
        }
        // Check for per-file @ts-check pragma
        if let Some(sf) = self.arena.source_files.first() {
            let text = sf.text.as_ref();
            let ts_check = text.find("@ts-check");
            let ts_no_check = text.find("@ts-nocheck");
            match (ts_check, ts_no_check) {
                (Some(check_idx), Some(no_check_idx)) => check_idx < no_check_idx,
                (Some(_), None) => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Check if noImplicitAny is enabled for the current file.
    /// For JavaScript files, noImplicitAny only applies when checkJs is also enabled.
    /// This allows TS7006 to fire in .js files with --checkJs --strict.
    pub fn no_implicit_any(&self) -> bool {
        if !self.compiler_options.no_implicit_any {
            return false;
        }

        // JS files get noImplicitAny errors only when checkJs is enabled
        if self.is_js_file() {
            self.compiler_options.check_js
        } else {
            true
        }
    }

    /// Check if noImplicitReturns is enabled.
    pub const fn no_implicit_returns(&self) -> bool {
        self.compiler_options.no_implicit_returns
    }

    /// Check if noImplicitThis is enabled.
    pub const fn no_implicit_this(&self) -> bool {
        self.compiler_options.no_implicit_this
    }

    /// Check if noImplicitOverride is enabled.
    pub const fn no_implicit_override(&self) -> bool {
        self.compiler_options.no_implicit_override
    }

    /// Check if strictNullChecks is enabled.
    pub const fn strict_null_checks(&self) -> bool {
        self.compiler_options.strict_null_checks
    }

    /// Check if strictFunctionTypes is enabled.
    pub const fn strict_function_types(&self) -> bool {
        self.compiler_options.strict_function_types
    }

    /// Check if strictPropertyInitialization is enabled.
    pub const fn strict_property_initialization(&self) -> bool {
        self.compiler_options.strict_property_initialization
    }

    /// Check if useUnknownInCatchVariables is enabled.
    pub const fn use_unknown_in_catch_variables(&self) -> bool {
        self.compiler_options.use_unknown_in_catch_variables
    }

    /// Check if isolatedModules is enabled.
    pub const fn isolated_modules(&self) -> bool {
        self.compiler_options.isolated_modules
    }

    /// Check if isolatedDeclarations is enabled.
    pub const fn isolated_declarations(&self) -> bool {
        self.compiler_options.isolated_declarations
    }

    /// Check if declaration emit is enabled.
    pub const fn emit_declarations(&self) -> bool {
        self.compiler_options.emit_declarations
    }

    /// Check if noUncheckedIndexedAccess is enabled.
    /// When enabled, index signature access adds `| undefined` to the result type.
    pub const fn no_unchecked_indexed_access(&self) -> bool {
        self.compiler_options.no_unchecked_indexed_access
    }

    /// Check if strictBindCallApply is enabled.
    /// When enabled, bind/call/apply use strict function signatures.
    pub const fn strict_bind_call_apply(&self) -> bool {
        self.compiler_options.strict_bind_call_apply
    }

    /// Check if exactOptionalPropertyTypes is enabled.
    /// When enabled, optional properties are `T | undefined` not `T | undefined | missing`.
    pub const fn exact_optional_property_types(&self) -> bool {
        self.compiler_options.exact_optional_property_types
    }

    /// Check if sound mode is enabled.
    pub const fn sound_mode(&self) -> bool {
        self.compiler_options.sound_mode
    }

    /// Pack the checker's compiler options into a `u16` bitmask for use as a
    /// `RelationCacheKey` flags field. This is the single source of truth for
    /// flag packing — call this instead of manually constructing the bitmask.
    pub const fn pack_relation_flags(&self) -> u16 {
        use crate::query_boundaries::assignability::RelationFlags;
        let mut flags: u16 = 0;
        if self.strict_null_checks() {
            flags |= RelationFlags::STRICT_NULL_CHECKS;
        }
        if self.strict_function_types() {
            flags |= RelationFlags::STRICT_FUNCTION_TYPES;
        }
        if self.exact_optional_property_types() {
            flags |= RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES;
        }
        if self.no_unchecked_indexed_access() {
            flags |= RelationFlags::NO_UNCHECKED_INDEXED_ACCESS;
        }
        flags
    }

    /// Convert `CheckerOptions` to `JudgeConfig` for the `CompatChecker`.
    const fn as_judge_config(&self) -> JudgeConfig {
        JudgeConfig {
            strict_function_types: self.strict_function_types(),
            strict_null_checks: self.strict_null_checks(),
            exact_optional_property_types: self.exact_optional_property_types(),
            no_unchecked_indexed_access: self.no_unchecked_indexed_access(),
            sound_mode: self.sound_mode(),
        }
    }

    /// Apply standard compiler options to a `CompatChecker`, including `query_db`.
    /// This wires the `CompilerOptions` (via `JudgeConfig`) and the `QueryDatabase`.
    pub fn configure_compat_checker<'b, R: tsz_solver::TypeResolver>(
        &'b self,
        checker: &mut tsz_solver::CompatChecker<'b, R>,
    ) {
        // Apply configuration from options
        checker.apply_config(&self.as_judge_config());

        // Set the query database for memoization/interning
        checker.set_query_db(self.types);

        // Set the inheritance graph for nominal class subtype checking
        checker.set_inheritance_graph(Some(&self.inheritance_graph));

        // Configure strict subtype checking if Sound Mode is enabled
        if self.compiler_options.sound_mode {
            checker.set_strict_subtype_checking(true);
            checker.set_strict_any_propagation(true);
        }
    }

    /// Check if noUnusedLocals is enabled.
    pub const fn no_unused_locals(&self) -> bool {
        self.compiler_options.no_unused_locals
    }

    /// Check if noUnusedParameters is enabled.
    pub const fn no_unused_parameters(&self) -> bool {
        self.compiler_options.no_unused_parameters
    }

    /// Check if noLib is enabled.
    /// When enabled, no library files (including lib.d.ts) are included.
    /// TS2318 errors are emitted when referencing global types with this option enabled.
    pub const fn no_lib(&self) -> bool {
        self.compiler_options.no_lib
    }

    /// Check if lib files are loaded (lib.d.ts, etc.).
    /// Returns false when noLib is enabled or when no actual lib files are loaded.
    /// Uses `actual_lib_file_count` instead of `lib_contexts.is_empty()` because `lib_contexts`
    /// may also contain user file contexts for cross-file resolution in multi-file tests.
    /// Used to determine whether to emit TS2304/TS2318/TS2583 for missing global types.
    pub const fn has_lib_loaded(&self) -> bool {
        !self.compiler_options.no_lib && self.actual_lib_file_count > 0
    }

    /// Check if esModuleInterop is enabled.
    /// When enabled, synthesizes default exports for `CommonJS` modules.
    pub const fn es_module_interop(&self) -> bool {
        self.compiler_options.es_module_interop
    }

    /// Check if allowSyntheticDefaultImports is enabled.
    /// When enabled, allows `import x from 'y'` when module doesn't have default export.
    /// This is implied by esModuleInterop (tsc treats esModuleInterop as enabling
    /// allowSyntheticDefaultImports automatically).
    pub const fn allow_synthetic_default_imports(&self) -> bool {
        self.compiler_options.allow_synthetic_default_imports
            || self.compiler_options.es_module_interop
    }
}
