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
    /// 2. Feature-specific or lib-version-specific globals when they should be
    ///    available but aren't: Awaited, `IterableIterator`,
    ///    `AsyncIterableIterator`, `TypedPropertyDescriptor`,
    ///    `CallableFunction`, `NewableFunction`, Disposable, `AsyncDisposable`
    ///
    /// This matches TypeScript's behavior in tests like noCrashOnNoLib.ts,
    /// generatorReturnTypeFallback.2.ts, missingDecoratorType.ts, etc.
    pub(crate) fn check_missing_global_types(&mut self) {
        // Core global types that TypeScript requires.
        // These are fundamental types that should always exist unless explicitly disabled.
        const CORE_GLOBAL_TYPES: &[&str] = &[
            "Array",
            "Boolean",
            "Function",
            "IArguments",
            "Number",
            "Object",
            "RegExp",
            "String",
        ];

        // CallableFunction/NewableFunction extend Function and provide better
        // typing for .call/.apply/.bind. tsc emits TS2318 for them when
        // Function itself is missing OR when --noLib is explicitly set (even
        // if Function is manually defined). With --noLib, the user is
        // responsible for all global types — omitting these is an error.
        const FUNCTION_AUX_TYPES: &[&str] = &["CallableFunction", "NewableFunction"];

        // Emit TS2318 errors when core global types are not available.
        // TypeScript always requires these core global types to exist.
        // tsc emits these errors BOTH with and without --noLib.
        //
        // However, when no lib files were loaded AND --noLib was not explicitly
        // set, we're likely in a bare unit test environment with no lib context.
        // Skip TS2318 emission UNLESS the file declares some core global types
        // manually (indicating the user intentionally set up a minimal-lib
        // environment and expects the check to run).
        if !self.ctx.capabilities.no_lib && !self.ctx.capabilities.has_lib {
            let has_any_core_type = CORE_GLOBAL_TYPES
                .iter()
                .any(|name| self.ctx.binder.file_locals.has(name));
            if !has_any_core_type {
                return;
            }
        }

        // We check if types exist globally (in libs or current file scope).
        // This matches tsc behavior where missing core types are reported
        // even when some libs are loaded (e.g., if --lib es6 is missing Array).
        for &type_name in CORE_GLOBAL_TYPES {
            // Check if the type is available in any loaded lib or current scope
            if !self.ctx.has_name_in_lib(type_name) {
                // Type not available globally - emit TS2318
                // tsc emits these with no file position (file="", line=0, column=0)
                self.error_global_type_missing_at_position(type_name, String::new(), 0, 0);
            }
        }

        // Check CallableFunction/NewableFunction when strictBindCallApply is enabled.
        // These types provide proper typing for .call()/.apply()/.bind().
        // TypeScript requires them whenever --noLib is explicitly set, even if the
        // user manually defines Function, because the user is then responsible for
        // the whole built-in global surface. Outside --noLib, tsc only requires them
        // when Function itself is also missing.
        if self.ctx.compiler_options.strict_bind_call_apply
            && (self.ctx.capabilities.no_lib || !self.ctx.has_name_in_lib("Function"))
        {
            for &type_name in FUNCTION_AUX_TYPES {
                if !self.ctx.has_name_in_lib(type_name) {
                    self.error_global_type_missing_at_position(type_name, String::new(), 0, 0);
                }
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
        let array_type_params_for_flow = array_type_params.clone();

        // Eagerly resolve ConcatArray and FlatArray, which are referenced by Array's
        // method signatures. Without registering these types' bodies in TypeEnvironment,
        // the solver's resolve_lazy falls through to a SymbolId-based fallback that can
        // produce wrong types due to DefId/SymbolId value collisions.
        // NOTE: ArrayIterator is NOT eagerly resolved here — it costs ~55ms due to deep
        // interface merging chains (ArrayIterator → IteratorObject → Iterator + Disposable
        // + esnext.iterator). Since the TypeInterner (DashMap) is shared across parallel
        // checkers, ArrayIterator is resolved lazily on first use and cached globally.
        for array_dep in &["ConcatArray", "FlatArray"] {
            let _ = self.resolve_lib_type_by_name(array_dep);
        }

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

        // If the user has augmented the Array interface (e.g.,
        // `interface Array<T> extends IFoo<T> {}`), re-resolve using
        // resolve_lib_type_by_name which processes global augmentation heritage
        // and type argument substitution. resolve_lib_type_with_params only reads
        // from lib binders and misses user augmentations.
        if self
            .ctx
            .binder
            .global_augmentations
            .get("Array")
            .is_some_and(|v| !v.is_empty())
            && let Some(augmented_type) = self.resolve_lib_type_by_name("Array")
        {
            self.ctx
                .types
                .register_array_base_type(augmented_type, array_type_params.clone());
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
                for ctx in self.ctx.lib_contexts.iter() {
                    if let Some(sym_id) = ctx.binder.file_locals.get(name) {
                        let def_id = self.ctx.get_lib_def_id(sym_id);
                        self.ctx.types.register_boxed_def_id(kind, def_id);
                    }
                }
                if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                    let def_id = self.ctx.get_lib_def_id(sym_id);
                    self.ctx.types.register_boxed_def_id(kind, def_id);
                }
            }
        }

        // Register ThisType marker DefIds so ThisTypeMarkerExtractor can identify
        // ThisType<T> applications when the base type is Lazy(DefId).
        for ctx in self.ctx.lib_contexts.iter() {
            if let Some(sym_id) = ctx.binder.file_locals.get("ThisType") {
                let def_id = self.ctx.get_lib_def_id(sym_id);
                self.ctx.types.register_this_type_def_id(def_id);
            }
        }
        if let Some(sym_id) = self.ctx.binder.file_locals.get("ThisType") {
            let def_id = self.ctx.get_lib_def_id(sym_id);
            self.ctx.types.register_this_type_def_id(def_id);
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
                    for ctx in self.ctx.lib_contexts.iter() {
                        if let Some(sym_id) = ctx.binder.file_locals.get(name) {
                            let def_id = self.ctx.get_lib_def_id(sym_id);
                            env.insert_def(def_id, ty);
                            env.register_boxed_def_id(kind, def_id);
                        }
                    }
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                        let def_id = self.ctx.get_lib_def_id(sym_id);
                        env.insert_def(def_id, ty);
                        env.register_boxed_def_id(kind, def_id);
                    }
                }
            }
        }

        // Mirror boxed DefId mappings into type_environment (flow-analyzer env)
        // so both environments stay consistent for narrowing contexts.
        if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
            for &(name, type_opt, kind) in boxed_names {
                if let Some(ty) = type_opt {
                    for ctx in self.ctx.lib_contexts.iter() {
                        if let Some(sym_id) = ctx.binder.file_locals.get(name) {
                            let def_id = self.ctx.get_lib_def_id(sym_id);
                            env.insert_def(def_id, ty);
                            env.register_boxed_def_id(kind, def_id);
                        }
                    }
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                        let def_id = self.ctx.get_lib_def_id(sym_id);
                        env.insert_def(def_id, ty);
                        env.register_boxed_def_id(kind, def_id);
                    }
                }
            }

            // Mirror boxed types and array base type into flow-analyzer env
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
            if let Some(ty) = array_instance_type {
                env.set_array_base_type(ty, array_type_params_for_flow);
            }
        }
    }

    /// Prime boxed and Array base types before checking files.
    ///
    /// Also calls `register_function_def_ids_early()` first, matching the
    /// file checker's DefId allocation order. Without this, the prime checker
    /// and file checkers would assign different `DefIds` to lib types like
    /// `ConcatArray`, causing Lazy(DefId) references in the interned Array body
    /// to resolve to wrong types.
    pub fn prime_boxed_types(&mut self) {
        self.register_function_def_ids_early();
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

        for ctx in self.ctx.lib_contexts.iter() {
            if let Some(sym_id) = ctx.binder.file_locals.get("Function") {
                let def_id = self.ctx.get_lib_def_id(sym_id);
                self.ctx
                    .types
                    .register_boxed_def_id(IntrinsicKind::Function, def_id);
            }
        }
        if let Some(sym_id) = self.ctx.binder.file_locals.get("Function") {
            let def_id = self.ctx.get_lib_def_id(sym_id);
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
    /// Routes through the capability boundary (`gate_for_required_type`) to map
    /// type names to feature gates, and `should_check_feature_gate` to determine
    /// whether the feature is actually used in the current file.
    ///
    /// Examples:
    /// - `TypedPropertyDescriptor`: Required for decorators
    /// - `IterableIterator`: Required for generators
    /// - `AsyncIterableIterator`: Required for async generators
    /// - Disposable/AsyncDisposable: Required for using declarations
    /// - Awaited: Required for await type operator
    pub(crate) fn check_feature_specific_global_types(&mut self) {
        use crate::query_boundaries::capabilities::EnvironmentCapabilities;

        // Feature-specific global types checked via the capability boundary.
        // The mapping from type name → feature gate is centralized in
        // `EnvironmentCapabilities::gate_for_required_type()`.
        const FEATURE_TYPES: &[&str] = &[
            "Awaited",
            "IterableIterator",
            "AsyncIterableIterator",
            "TypedPropertyDescriptor",
            "Disposable",
            "AsyncDisposable",
        ];

        for &type_name in FEATURE_TYPES {
            // Check if available in lib contexts or declared locally
            if self.ctx.has_name_in_lib(type_name) || self.ctx.binder.file_locals.has(type_name) {
                continue;
            }

            // Disposable/AsyncDisposable: tsc only emits TS2318 when the target
            // requires downleveling of `using`/`await using` (target < ES2025).
            // When native support is available (target >= ES2025/ESNext), the types
            // are only needed if they happen to be in the lib; their absence is not
            // an error.
            if matches!(type_name, "Disposable" | "AsyncDisposable")
                && self.ctx.compiler_options.target.supports_es2025()
            {
                continue;
            }

            // Use the capability boundary to map the type to its feature gate
            let Some(gate) = EnvironmentCapabilities::gate_for_required_type(type_name) else {
                continue;
            };

            // Only emit if the feature is actually used in this file
            if !self.should_check_feature_gate(gate) {
                continue;
            }

            // tsc emits these with no file position (file="", line=0, column=0)
            self.error_global_type_missing_at_position(type_name, String::new(), 0, 0);
        }
    }

    /// Check if a feature gate's corresponding syntax is used in the current file.
    ///
    /// This heuristic determines if a feature that requires a specific global type
    /// is likely being used in the code. These errors are NOT emitted just because
    /// noLib is set — they require the actual feature to be used.
    ///
    /// Routes through `FileFeatures` flags set by the binder, and checker-level
    /// state for async depth.
    pub(crate) const fn should_check_feature_gate(
        &self,
        gate: crate::query_boundaries::capabilities::FeatureGate,
    ) -> bool {
        use crate::query_boundaries::capabilities::FeatureGate;
        use tsz_binder::FileFeatures;
        let features = self.ctx.binder.file_features;
        match gate {
            FeatureGate::Generators => features.has(FileFeatures::GENERATORS),
            FeatureGate::AsyncGenerators => features.has(FileFeatures::ASYNC_GENERATORS),
            FeatureGate::ExperimentalDecorators => {
                self.ctx.compiler_options.experimental_decorators
                    && features.has(FileFeatures::DECORATORS)
            }
            FeatureGate::UsingDeclaration => features.has(FileFeatures::USING),
            FeatureGate::AwaitUsingDeclaration => features.has(FileFeatures::AWAIT_USING),
            // Awaited maps to AsyncFunction gate — check async_depth
            FeatureGate::AsyncFunction => self.ctx.async_depth > 0,
            _ => false,
        }
    }
}
