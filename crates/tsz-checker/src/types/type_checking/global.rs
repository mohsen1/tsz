//! Global type checking: missing types, boxed types.
//!
//! This module extends `CheckerState` with methods for global-scope checking:
//! - Checking for missing global types (TS2318)
//! - Registering and priming boxed types
//! - Checking for feature-specific global types
//!
//! Duplicate identifier checking lives in `type_checking/duplicate_identifiers`.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check for missing global types (TS2318).
    ///
    /// When library files are not loaded or specific global types are unavailable,
    /// TypeScript emits TS2318 errors for essential global types at the beginning
    /// of the file (position 0).
    ///
    /// This function checks for:
    /// 1. Core 8 types when --noLib is used: Array, Boolean, Function, `IArguments`,
    ///    Number, Object, `RegExp`, String
    /// 2. ES2015+ types when they should be available but aren't: Awaited,
    ///    `IterableIterator`, `AsyncIterableIterator`, `TypedPropertyDescriptor`,
    ///    `CallableFunction`, `NewableFunction`, Disposable, `AsyncDisposable`
    ///
    /// This matches TypeScript's behavior in tests like noCrashOnNoLib.ts,
    /// generatorReturnTypeFallback.2.ts, missingDecoratorType.ts, etc.
    pub(crate) fn check_missing_global_types(&mut self) {
        use tsz_binder::lib_loader;

        // Core global types that TypeScript requires.
        // These are fundamental types that should always exist unless explicitly disabled.
        const CORE_GLOBAL_TYPES: &[&str] = &[
            "Array",
            "Boolean",
            "CallableFunction",
            "Function",
            "IArguments",
            "NewableFunction",
            "Number",
            "Object",
            "RegExp",
            "String",
        ];

        // Emit TS2318 errors when core global types are not available.
        // TypeScript always requires these core global types to exist.
        // tsc emits these errors BOTH with and without --noLib.
        //
        // We check if types exist globally (in libs or current file scope).
        // This matches tsc behavior where missing core types are reported
        // even when some libs are loaded (e.g., if --lib es6 is missing Array).
        for &type_name in CORE_GLOBAL_TYPES {
            // Check if the type is available in any loaded lib or current scope
            if !self.ctx.has_name_in_lib(type_name) {
                // Type not available globally - emit TS2318
                // tsc emits these with no file position (file="", line=0, column=0)
                self.ctx
                    .push_diagnostic(lib_loader::emit_error_global_type_missing(
                        type_name,
                        String::new(),
                        0,
                        0,
                    ));
            }
        }

        // Check for feature-specific global types that may be missing
        // These are checked regardless of --noLib, but only if the feature appears to be used
        self.check_feature_specific_global_types();
    }

    /// Register boxed types (String, Number, Boolean, etc.) from lib.d.ts in `TypeEnvironment`.
    ///
    /// This enables primitive property access to use lib.d.ts definitions instead of
    /// hardcoded lists. For example, "foo".length will look up the String interface
    /// from lib.d.ts and find the length property there.
    pub(crate) fn register_boxed_types(&mut self) {
        use tsz_solver::IntrinsicKind;

        // Only register if lib files are loaded
        if !self.ctx.has_lib_loaded() {
            return;
        }

        // 1. Resolve types first (avoids holding a mutable borrow on type_env while resolving)
        // resolve_lib_type_by_name handles looking up in lib.d.ts and merging declarations
        let string_type = self.resolve_lib_type_by_name("String");
        let number_type = self.resolve_lib_type_by_name("Number");
        let boolean_type = self.resolve_lib_type_by_name("Boolean");
        let symbol_type = self.resolve_lib_type_by_name("Symbol");
        let bigint_type = self.resolve_lib_type_by_name("BigInt");
        let object_type = self.resolve_lib_type_by_name("Object");
        let function_type = self.resolve_lib_type_by_name("Function");

        // For Array<T>, extract the actual type parameters from the interface definition
        // rather than synthesizing fresh ones. This ensures the T used in Array's method
        // signatures has the same TypeId as the T registered in TypeEnvironment.
        let (array_type, array_type_params) = self.resolve_lib_type_with_params("Array");

        // Pre-compute type parameters for commonly-used generic lib types.
        // To reduce startup overhead, only prewarm symbols referenced by this file.
        // Unreferenced symbols are still resolved lazily through normal lookup paths.
        let mut referenced_type_names = FxHashSet::default();
        for idx in 0..self.ctx.arena.len() {
            let node_idx = NodeIndex(idx as u32);
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };
            if node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(identifier) = self.ctx.arena.get_identifier(node)
            {
                referenced_type_names.insert(identifier.escaped_text.clone());
            }
        }

        for type_name in &[
            "ReadonlyArray",
            "Promise",
            "PromiseLike",
            "Awaited",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "WeakRef",
            "ReadonlyMap",
            "ReadonlySet",
            "Iterator",
            "IterableIterator",
            "AsyncIterator",
            "AsyncIterable",
            "AsyncIterableIterator",
            "Generator",
            "AsyncGenerator",
            "Partial",
            "Required",
            "Readonly",
            "Record",
            "Pick",
            "Omit",
            "Exclude",
            "Extract",
            "NonNullable",
            "ReturnType",
            "Parameters",
            "ConstructorParameters",
            "InstanceType",
            "ThisParameterType",
            "OmitThisParameter",
        ] {
            if referenced_type_names.contains(*type_name) {
                self.prime_lib_type_params(type_name);
            }
        }

        // The Array type from lib.d.ts is a Callable with instance methods as properties
        // We register this type directly so that resolve_array_property can use it
        // No need to extract instance type from construct signatures - the methods
        // are already on the Callable itself
        let array_instance_type = array_type;

        // PropertyAccessEvaluator runs through multiple database backends
        // (query cache, interner, binder-backed resolver). Register Array<T>
        // through the query database so all backends see the same base type.
        if let Some(ty) = array_instance_type {
            self.ctx
                .types
                .register_array_base_type(ty, array_type_params.clone());
        }

        // Register boxed types through the query database so PropertyAccessEvaluator
        // can resolve primitive methods (e.g., "hello".match()) through the actual
        // interface types from lib.d.ts instead of falling back to hardcoded lists.
        let boxed_pairs: &[(IntrinsicKind, Option<TypeId>)] = &[
            (IntrinsicKind::String, string_type),
            (IntrinsicKind::Number, number_type),
            (IntrinsicKind::Boolean, boolean_type),
            (IntrinsicKind::Symbol, symbol_type),
            (IntrinsicKind::Bigint, bigint_type),
            (IntrinsicKind::Object, object_type),
            (IntrinsicKind::Function, function_type),
        ];
        for &(kind, type_id) in boxed_pairs {
            if let Some(ty) = type_id {
                self.ctx.types.register_boxed_type(kind, ty);
                // Also register the DefId (if it's a Lazy type) so the interner
                // can identify boxed types by DefId even when TypeEnvironment
                // is unavailable (e.g., during RefCell borrow conflicts).
                if let Some(def_id) =
                    tsz_solver::visitor::lazy_def_id(self.ctx.types.as_type_database(), ty)
                {
                    self.ctx.types.register_boxed_def_id(kind, def_id);
                }
            }
        }

        // Register DefIds from ALL lib contexts in the interner EAGERLY.
        // This must happen before any constraint checking (which may occur during
        // build_type_environment), so the SubtypeChecker and generic constraint
        // validation can identify boxed types by DefId even before the
        // TypeEnvironment is populated.
        let boxed_names: &[(&str, Option<TypeId>, IntrinsicKind)] = &[
            ("String", string_type, IntrinsicKind::String),
            ("Number", number_type, IntrinsicKind::Number),
            ("Boolean", boolean_type, IntrinsicKind::Boolean),
            ("Symbol", symbol_type, IntrinsicKind::Symbol),
            ("BigInt", bigint_type, IntrinsicKind::Bigint),
            ("Object", object_type, IntrinsicKind::Object),
            ("Function", function_type, IntrinsicKind::Function),
        ];
        for &(name, type_opt, kind) in boxed_names {
            if type_opt.is_some() {
                for ctx in &self.ctx.lib_contexts {
                    if let Some(sym_id) = ctx.binder.file_locals.get(name) {
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        self.ctx.types.register_boxed_def_id(kind, def_id);
                    }
                }
                if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    self.ctx.types.register_boxed_def_id(kind, def_id);
                }
            }
        }

        // 2. Populate the environment
        // We use try_borrow_mut to be safe, though at this stage it should be free
        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            if let Some(ty) = string_type {
                env.set_boxed_type(IntrinsicKind::String, ty);
            }
            if let Some(ty) = number_type {
                env.set_boxed_type(IntrinsicKind::Number, ty);
            }
            if let Some(ty) = boolean_type {
                env.set_boxed_type(IntrinsicKind::Boolean, ty);
            }
            if let Some(ty) = symbol_type {
                env.set_boxed_type(IntrinsicKind::Symbol, ty);
            }
            if let Some(ty) = bigint_type {
                env.set_boxed_type(IntrinsicKind::Bigint, ty);
            }
            if let Some(ty) = object_type {
                env.set_boxed_type(IntrinsicKind::Object, ty);
            }
            if let Some(ty) = function_type {
                env.set_boxed_type(IntrinsicKind::Function, ty);
            }
            // Register the Array<T> interface for array property resolution
            // Use the instance type (Array<T> interface), not the constructor (Callable)
            if let Some(ty) = array_instance_type {
                env.set_array_base_type(ty, array_type_params);
            }

            // 3. Register DefId mappings for non-generic boxed types in the env too.
            // When user code writes `a: Function`, the type annotation creates a
            // Lazy(DefId) referencing the global Function symbol. The CallEvaluator
            // uses TypeEnvironment as its resolver, which resolves Lazy types via
            // def_types. Without this registration, Lazy(DefId) for Function can't
            // be resolved, causing false TS2345/TS2322 errors.
            for &(name, type_opt, kind) in boxed_names {
                if let Some(ty) = type_opt {
                    for ctx in &self.ctx.lib_contexts {
                        if let Some(sym_id) = ctx.binder.file_locals.get(name) {
                            let def_id = self.ctx.get_or_create_def_id(sym_id);
                            env.insert_def(def_id, ty);
                            env.register_boxed_def_id(kind, def_id);
                        }
                    }
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        env.insert_def(def_id, ty);
                        env.register_boxed_def_id(kind, def_id);
                    }
                }
            }
        }
    }

    /// Prime boxed and Array base types before checking files.
    pub fn prime_boxed_types(&mut self) {
        self.register_boxed_types();
    }

    /// Early-register Function interface `DefIds` in the interner (`DashMap`).
    ///
    /// This must be called BEFORE `build_type_environment()` so that constraint
    /// checks during type alias processing (e.g., `T extends Function`) can
    /// identify the Function interface. Only registers Function to minimize
    /// side effects on DefId creation ordering.
    pub(crate) fn register_function_def_ids_early(&mut self) {
        use tsz_solver::IntrinsicKind;

        if !self.ctx.has_lib_loaded() {
            return;
        }

        for ctx in &self.ctx.lib_contexts {
            if let Some(sym_id) = ctx.binder.file_locals.get("Function") {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                self.ctx
                    .types
                    .register_boxed_def_id(IntrinsicKind::Function, def_id);
            }
        }
        if let Some(sym_id) = self.ctx.binder.file_locals.get("Function") {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx
                .types
                .register_boxed_def_id(IntrinsicKind::Function, def_id);
        }
    }

    /// Check for feature-specific global types that may be missing.
    ///
    /// This function checks if certain global types that are required for specific
    /// TypeScript features are available. Unlike the core global types, these are
    /// only checked when the feature is potentially used in the code.
    ///
    /// Examples:
    /// - `TypedPropertyDescriptor`: Required for decorators
    /// - `IterableIterator`: Required for generators
    /// - `AsyncIterableIterator`: Required for async generators
    /// - Disposable/AsyncDisposable: Required for using declarations
    /// - Awaited: Required for await type operator
    pub(crate) fn check_feature_specific_global_types(&mut self) {
        use tsz_binder::lib_loader;

        // Types that are commonly referenced in TypeScript features
        // We check if these are available in lib contexts
        let feature_types = [
            // ES2015+ types that are commonly needed
            ("Awaited", "ES2022"),               // For await type operator
            ("IterableIterator", "ES2015"),      // For generators
            ("AsyncIterableIterator", "ES2018"), // For async generators
            ("TypedPropertyDescriptor", "ES5"),  // For decorators
            ("Disposable", "ES2022"),            // For using declarations
            ("AsyncDisposable", "ES2022"),       // For await using declarations
        ];

        for &(type_name, _es_version) in &feature_types {
            // Check if the type should be available but isn't
            // Only check if:
            // 1. The type is not in lib contexts (not available from loaded libs)
            // 2. The type is not declared in the current file
            // 3. This appears to be a scenario where the type would be referenced

            // Check if available in lib contexts
            if self.ctx.has_name_in_lib(type_name) {
                continue; // Type is available
            }

            // Check if declared in current file
            if self.ctx.binder.file_locals.has(type_name) {
                continue; // Type is declared locally
            }

            // At this point, the type is not available
            // TypeScript emits TS2318 at position 0 if the type would be referenced
            // For now, we'll emit based on certain heuristics:

            let should_emit = match type_name {
                // Always check these when libs are minimal (ES5 or noLib)
                "IterableIterator"
                | "AsyncIterableIterator"
                | "TypedPropertyDescriptor"
                | "Disposable"
                | "AsyncDisposable" => {
                    // These are emitted when the feature syntax is detected
                    // For simplicity, we check if any syntax that would need them exists
                    self.should_check_for_feature_type(type_name)
                }
                // Awaited is checked when using await type operator or async functions
                "Awaited" => {
                    // TSC emits TS2318 for Awaited when async/await syntax is used
                    self.ctx.async_depth > 0
                }
                _ => false,
            };

            if should_emit {
                // tsc emits these with no file position (file="", line=0, column=0)
                let diag =
                    lib_loader::emit_error_global_type_missing(type_name, String::new(), 0, 0);
                // Use push_diagnostic for consistent deduplication
                self.ctx.push_diagnostic(diag);
            }
        }
    }

    /// Check if we should emit an error for a feature-specific global type.
    ///
    /// This heuristic determines if a feature that requires a specific global type
    /// is likely being used in the code. These errors are NOT emitted just because
    /// noLib is set — they require the actual feature to be used.
    pub(crate) fn should_check_for_feature_type(&self, type_name: &str) -> bool {
        use tsz_binder::FileFeatures;
        let features = self.ctx.binder.file_features;
        match type_name {
            "IterableIterator" => features.has(FileFeatures::GENERATORS),
            "AsyncIterableIterator" => features.has(FileFeatures::ASYNC_GENERATORS),
            "TypedPropertyDescriptor" => {
                self.ctx.compiler_options.experimental_decorators
                    && features.has(FileFeatures::DECORATORS)
            }
            "Disposable" => features.has(FileFeatures::USING),
            "AsyncDisposable" => features.has(FileFeatures::AWAIT_USING),
            _ => false,
        }
    }
}
