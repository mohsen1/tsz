//! Global type checking: missing types, boxed types, duplicate identifiers, unused declarations.
//!
//! This module extends `CheckerState` with methods for global-scope checking:
//! - Checking for missing global types (TS2318)
//! - Registering and priming boxed types
//! - Checking for duplicate identifier declarations (TS2300, TS2301, etc.)
//! - Checking for unused declarations (TS6133, etc.)

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
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
            "Function",
            "IArguments",
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
                self.ctx
                    .push_diagnostic(lib_loader::emit_error_global_type_missing(
                        type_name,
                        self.ctx.file_name.clone(),
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
        // This populates the def_type_params cache so that:
        // 1. validate_type_reference_type_arguments can check constraints (TS2344)
        // 2. Application(Lazy(DefId), Args) expansion works in the solver
        // Without this, cross-arena delegation in get_type_params_for_symbol fails
        // for lib symbols due to depth guards, causing constraint checks to be skipped.
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
            // resolve_lib_type_with_params internally caches type params via
            // insert_def_type_params, making them available for constraint checking
            let _ = self.resolve_lib_type_with_params(type_name);
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
        for (kind, type_id) in [
            (IntrinsicKind::String, string_type),
            (IntrinsicKind::Number, number_type),
            (IntrinsicKind::Boolean, boolean_type),
            (IntrinsicKind::Symbol, symbol_type),
            (IntrinsicKind::Bigint, bigint_type),
            (IntrinsicKind::Object, object_type),
            (IntrinsicKind::Function, function_type),
        ] {
            if let Some(ty) = type_id {
                self.ctx.types.register_boxed_type(kind, ty);
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

            // 3. Register DefId mappings for non-generic boxed types.
            // When user code writes `a: Function`, the type annotation creates a
            // Lazy(DefId) referencing the global Function symbol. The CallEvaluator
            // uses TypeEnvironment as its resolver, which resolves Lazy types via
            // def_types. Without this registration, Lazy(DefId) for Function can't
            // be resolved, causing false TS2345/TS2322 errors.
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
                if let Some(ty) = type_opt {
                    // Register DefIds from ALL lib contexts, not just the first.
                    // Multiple lib files (es5, es2015, etc.) each have their own
                    // symbol for types like Function, String, etc. User code can
                    // reference any of them, so all must resolve to the same type.
                    for ctx in &self.ctx.lib_contexts {
                        if let Some(sym_id) = ctx.binder.file_locals.get(name) {
                            let def_id = self.ctx.get_or_create_def_id(sym_id);
                            env.insert_def(def_id, ty);
                            env.register_boxed_def_id(kind, def_id);
                        }
                    }
                    // Also register from current file's binder (for global augmentations)
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
            ("CallableFunction", "ES2015"),      // For strict function types
            ("NewableFunction", "ES2015"),       // For constructor types
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
                // Awaited is checked when using await type operator, async functions, or Promise-like types
                "Awaited" => {
                    // TSC emits TS2318 for Awaited when Promise-like types are used, even without explicit await
                    // Check if async/await is used OR if noLib is true (TSC checks it in that case)
                    self.ctx.async_depth > 0 || self.ctx.no_lib()
                }
                _ => false,
            };

            if should_emit {
                let diag = lib_loader::emit_error_global_type_missing(
                    type_name,
                    self.ctx.file_name.clone(),
                    0,
                    0,
                );
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

    /// Check for duplicate identifiers (TS2300, TS2451, TS2392).
    /// Reports when variables, functions, classes, or other declarations
    /// have conflicting names within the same scope.
    pub(crate) fn check_duplicate_identifiers(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use rustc_hash::FxHashMap;

        // When lib contexts are loaded, skip symbols that come from lib files.
        // Lib types (Array, String, etc.) have multiple declarations from merged
        // lib files which are not actual duplicates.
        let has_libs = self.ctx.has_lib_loaded();

        let mut symbol_ids = FxHashSet::default();
        if !self.ctx.binder.scopes.is_empty() {
            for scope in &self.ctx.binder.scopes {
                // Skip class scopes - class member duplicates need specialized handling
                // (static vs instance separation, method overloads, get/set pairs, etc.)
                if scope.kind == tsz_binder::ContainerKind::Class {
                    continue;
                }
                for (_, &id) in scope.table.iter() {
                    symbol_ids.insert(id);
                }
            }
        } else {
            for (_, &id) in self.ctx.binder.file_locals.iter() {
                symbol_ids.insert(id);
            }
        }

        for sym_id in symbol_ids {
            // Skip symbols that come from lib files - they have multiple declarations
            // from different lib files (e.g. lib.es5.d.ts, lib.es2015.core.d.ts) that
            // are not actual duplicates.
            if has_libs && self.ctx.symbol_is_from_lib(sym_id) {
                continue;
            }

            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            if symbol.declarations.len() <= 1 {
                continue;
            }

            // Handle constructors separately - they use TS2392 (multiple constructor implementations), not TS2300
            if symbol.escaped_name == "constructor" {
                // Count only constructor implementations (with body), not overloads (without body)
                let implementations: Vec<NodeIndex> = symbol
                    .declarations
                    .iter()
                    .filter_map(|&decl_idx| {
                        let constructor = self.ctx.arena.get_constructor_at(decl_idx)?;
                        // Only count constructors with a body as implementations
                        (!constructor.body.is_none()).then_some(decl_idx)
                    })
                    .collect();

                // Report TS2392 for multiple constructor implementations (not overloads)
                if implementations.len() > 1 {
                    let message =
                        diagnostic_messages::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED;
                    for &decl_idx in &implementations {
                        self.error_at_node(
                            decl_idx,
                            message,
                            diagnostic_codes::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED,
                        );
                    }
                }
                continue;
            }

            let mut declarations = Vec::new();
            for &decl_idx in &symbol.declarations {
                if let Some(flags) = self.declaration_symbol_flags(decl_idx) {
                    // When libs are loaded, verify the declaration name matches the symbol.
                    // Lib declarations may have NodeIndex values that overlap with user arena
                    // indices, pointing to unrelated user nodes. Filter these out.
                    if has_libs && !self.declaration_name_matches(decl_idx, &symbol.escaped_name) {
                        continue;
                    }
                    declarations.push((decl_idx, flags));
                }
            }

            if declarations.len() <= 1 {
                continue;
            }

            // TS2395: Individual declarations in merged declaration must be all exported or all local.
            // When TS2395 fires, we skip the TS2300/TS2323 check for those declarations since
            // the root cause is export visibility mismatch, not a true duplicate name.
            let mut has_ts2395 = false;
            // Uses "declaration spaces" (Type=1, Value=2, Namespace=4) to determine if exported
            // and non-exported declarations overlap in the same semantic space.
            // Declarations must be grouped by their enclosing namespace body (or file scope)
            // since declarations in different namespace blocks of a merged namespace are separate.
            // Skip for ambient contexts (declare namespace, .d.ts) and pure function overloads.
            {
                const SPACE_TYPE: u32 = 1;
                const SPACE_VALUE: u32 = 2;
                const SPACE_NAMESPACE: u32 = 4;

                // Skip if any declaration is in an ambient context — ambient declarations
                // (declare namespace, declare module, .d.ts files) allow mixed export visibility.
                // We check specifically for declare namespace/module ancestors, not the general
                // is_ambient_declaration which also treats interfaces/type aliases as ambient.
                let any_in_declare_context = self.ctx.file_name.ends_with(".d.ts")
                    || declarations
                        .iter()
                        .any(|&(decl_idx, _)| self.is_in_declare_namespace_or_module(decl_idx));

                let mut error_nodes: Vec<NodeIndex> = Vec::new();

                if !any_in_declare_context {
                    // Pre-compute declaration spaces, export status, and enclosing scope
                    let decl_info: Vec<(NodeIndex, u32, u32, bool, NodeIndex)> = declarations
                        .iter()
                        .map(|&(decl_idx, flags)| {
                            let space = if (flags & symbol_flags::INTERFACE) != 0
                                || (flags & symbol_flags::TYPE_ALIAS) != 0
                            {
                                SPACE_TYPE
                            } else if (flags
                                & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                                != 0
                            {
                                if self.is_namespace_declaration_instantiated(decl_idx) {
                                    SPACE_NAMESPACE | SPACE_VALUE
                                } else {
                                    SPACE_NAMESPACE
                                }
                            } else if (flags & symbol_flags::CLASS) != 0
                                || (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                                    != 0
                            {
                                SPACE_TYPE | SPACE_VALUE
                            } else if (flags & symbol_flags::VARIABLE) != 0
                                || (flags & symbol_flags::FUNCTION) != 0
                            {
                                SPACE_VALUE
                            } else {
                                0
                            };
                            let exported = self.is_declaration_exported(decl_idx);
                            let scope = self.get_enclosing_namespace(decl_idx);
                            (decl_idx, flags, space, exported, scope)
                        })
                        .collect();

                    // Group by enclosing scope and check each group
                    type ScopeGroupEntry = (NodeIndex, u32, u32, bool);
                    let mut scope_groups: FxHashMap<NodeIndex, Vec<ScopeGroupEntry>> =
                        FxHashMap::default();
                    for &(decl_idx, flags, space, exported, scope) in &decl_info {
                        scope_groups
                            .entry(scope)
                            .or_default()
                            .push((decl_idx, flags, space, exported));
                    }

                    for group in scope_groups.values() {
                        if group.len() <= 1 {
                            continue;
                        }
                        // Skip groups where all declarations are functions — mixed export
                        // on function overloads is handled by TS2383-2386 instead.
                        let all_functions = group
                            .iter()
                            .all(|&(_, flags, _, _)| (flags & symbol_flags::FUNCTION) != 0);
                        if all_functions {
                            continue;
                        }
                        let mut exported_spaces: u32 = 0;
                        let mut non_exported_spaces: u32 = 0;
                        for &(_, _, space, exported) in group {
                            if exported {
                                exported_spaces |= space;
                            } else {
                                non_exported_spaces |= space;
                            }
                        }
                        let common_spaces = exported_spaces & non_exported_spaces;
                        if common_spaces != 0 {
                            has_ts2395 = true;
                            for &(decl_idx, _, space, _) in group {
                                if (space & common_spaces) != 0 {
                                    let error_node = self
                                        .get_declaration_name_node(decl_idx)
                                        .unwrap_or(decl_idx);
                                    error_nodes.push(error_node);
                                }
                            }
                        }
                    }
                }

                if has_ts2395 {
                    let name = symbol.escaped_name.clone();
                    let message = format_message(
                        diagnostic_messages::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                        &[&name],
                    );
                    for error_node in error_nodes {
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                        );
                    }
                }
            }

            // TS2428: interface merges must have identical type parameters.
            let interface_decls: Vec<NodeIndex> = declarations
                .iter()
                .filter(|(_, flags)| (flags & symbol_flags::INTERFACE) != 0)
                .map(|(decl_idx, _)| *decl_idx)
                .collect();
            if interface_decls.len() > 1 {
                let mut interface_decls_by_scope: FxHashMap<NodeIndex, Vec<NodeIndex>> =
                    FxHashMap::default();
                for &decl_idx in &interface_decls {
                    let scope = self.get_enclosing_namespace(decl_idx);
                    interface_decls_by_scope
                        .entry(scope)
                        .or_default()
                        .push(decl_idx);
                }

                for decls_in_scope in interface_decls_by_scope.into_values() {
                    if decls_in_scope.len() <= 1 {
                        continue;
                    }

                    self.check_merged_interface_declaration_diagnostics(&decls_in_scope);

                    let mismatch =
                        decls_in_scope
                            .as_slice()
                            .split_first()
                            .is_some_and(|(baseline, rest)| {
                                rest.iter().any(|&decl_idx| {
                                    !self.interface_type_parameters_are_merge_compatible(
                                        *baseline, decl_idx,
                                    )
                                })
                            });
                    if mismatch {
                        let message = format_message(
                            diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                            &[&symbol.escaped_name],
                        );
                        for decl_idx in decls_in_scope {
                            let error_node =
                                self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                            self.error_at_node(
                                error_node,
                                &message,
                                diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                            );
                        }
                    }
                }
            }

            self.check_merged_enum_declaration_diagnostics(&declarations);

            let mut conflicts = FxHashSet::default();
            let mut namespace_order_errors = FxHashSet::default();
            for i in 0..declarations.len() {
                for j in (i + 1)..declarations.len() {
                    let (decl_idx, decl_flags) = declarations[i];
                    let (other_idx, other_flags) = declarations[j];

                    // Skip conflict check if declarations are in different files
                    // (external modules are isolated, same-name declarations don't conflict)
                    // We check if both declarations are in the current file's arena
                    let both_in_current_file = self.ctx.arena.get(decl_idx).is_some()
                        && self.ctx.arena.get(other_idx).is_some();

                    // If either declaration is not in the current file's arena, they can't conflict
                    // This handles external modules where declarations in different files are isolated
                    if !both_in_current_file {
                        continue;
                    }

                    // Check for function overloads - multiple function declarations are allowed
                    // if at most one of them has a body (is an implementation)
                    let both_functions = (decl_flags & symbol_flags::FUNCTION) != 0
                        && (other_flags & symbol_flags::FUNCTION) != 0;
                    if both_functions {
                        let decl_has_body = self.function_has_body(decl_idx);
                        let other_has_body = self.function_has_body(other_idx);
                        // Only conflict if BOTH have bodies (multiple implementations)
                        if !(decl_has_body && other_has_body) {
                            continue;
                        }
                        // Both have bodies -> duplicate function implementations
                        // Force-add to conflicts since declarations_conflict returns false
                        // for FUNCTION vs FUNCTION (they don't exclude each other).
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                        continue;
                    }

                    // Check for method overloads - multiple method declarations are allowed
                    // if at most one of them has a body (is an implementation)
                    let both_methods = (decl_flags & symbol_flags::METHOD) != 0
                        && (other_flags & symbol_flags::METHOD) != 0;
                    if both_methods {
                        let decl_has_body = self.method_has_body(decl_idx);
                        let other_has_body = self.method_has_body(other_idx);
                        // Only conflict if BOTH have bodies (multiple implementations)
                        if !(decl_has_body && other_has_body) {
                            continue;
                        }
                    }

                    // Check for interface merging - multiple interface declarations are allowed
                    let both_interfaces = (decl_flags & symbol_flags::INTERFACE) != 0
                        && (other_flags & symbol_flags::INTERFACE) != 0;
                    if both_interfaces {
                        continue; // Interface merging is always allowed
                    }

                    // Check for enum merging - multiple enum declarations are allowed
                    let both_enums = (decl_flags & symbol_flags::ENUM) != 0
                        && (other_flags & symbol_flags::ENUM) != 0;
                    if both_enums {
                        continue; // Enum merging is always allowed
                    }

                    // Check for namespace merging - namespaces can merge with functions, classes, and each other
                    let decl_is_namespace = (decl_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;
                    let other_is_namespace = (other_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0;

                    // Namespace + Namespace merging is allowed
                    if decl_is_namespace && other_is_namespace {
                        continue;
                    }

                    // Namespace + Function merging is allowed only when the namespace
                    // is non-instantiated OR declared after the function.
                    let decl_is_function = (decl_flags & symbol_flags::FUNCTION) != 0;
                    let other_is_function = (other_flags & symbol_flags::FUNCTION) != 0;
                    if (decl_is_namespace && other_is_function)
                        || (decl_is_function && other_is_namespace)
                    {
                        let (namespace_idx, function_idx) = if decl_is_namespace {
                            (decl_idx, other_idx)
                        } else {
                            (other_idx, decl_idx)
                        };

                        let namespace_is_instantiated =
                            self.is_namespace_declaration_instantiated(namespace_idx);
                        if !namespace_is_instantiated {
                            continue;
                        }

                        if self.is_ambient_function_declaration(function_idx) {
                            continue;
                        }

                        let namespace_precedes_function = self
                            .ctx
                            .arena
                            .get(namespace_idx)
                            .zip(self.ctx.arena.get(function_idx))
                            .is_some_and(|(ns_node, fn_node)| ns_node.pos < fn_node.pos);

                        if namespace_precedes_function {
                            namespace_order_errors.insert(namespace_idx);
                        }
                        continue;
                    }

                    // Namespace + Class merging is allowed only when the namespace
                    // is non-instantiated OR declared after the class.
                    let decl_is_class = (decl_flags & symbol_flags::CLASS) != 0;
                    let other_is_class = (other_flags & symbol_flags::CLASS) != 0;
                    if (decl_is_namespace && other_is_class)
                        || (decl_is_class && other_is_namespace)
                    {
                        let (namespace_idx, class_idx) = if decl_is_namespace {
                            (decl_idx, other_idx)
                        } else {
                            (other_idx, decl_idx)
                        };

                        let namespace_is_instantiated =
                            self.is_namespace_declaration_instantiated(namespace_idx);
                        if !namespace_is_instantiated {
                            continue;
                        }

                        if self.is_ambient_class_declaration(class_idx) {
                            continue;
                        }

                        let namespace_precedes_class = self
                            .ctx
                            .arena
                            .get(namespace_idx)
                            .zip(self.ctx.arena.get(class_idx))
                            .is_some_and(|(ns_node, class_node)| ns_node.pos < class_node.pos);

                        if namespace_precedes_class {
                            namespace_order_errors.insert(namespace_idx);
                        }
                        continue;
                    }

                    // Namespace + Enum merging is allowed
                    let decl_is_enum = (decl_flags & symbol_flags::ENUM) != 0;
                    let other_is_enum = (other_flags & symbol_flags::ENUM) != 0;
                    if (decl_is_namespace && other_is_enum) || (decl_is_enum && other_is_namespace)
                    {
                        continue;
                    }

                    // Namespace + Variable merging is allowed only for non-instantiated
                    // namespaces. Instantiated namespaces conflict with variables.
                    let decl_is_variable = (decl_flags & symbol_flags::VARIABLE) != 0;
                    let other_is_variable = (other_flags & symbol_flags::VARIABLE) != 0;
                    if (decl_is_namespace && other_is_variable)
                        || (decl_is_variable && other_is_namespace)
                    {
                        let namespace_idx = if decl_is_namespace {
                            decl_idx
                        } else {
                            other_idx
                        };
                        let namespace_is_instantiated =
                            self.is_namespace_declaration_instantiated(namespace_idx);
                        if namespace_is_instantiated {
                            conflicts.insert(decl_idx);
                            conflicts.insert(other_idx);
                        }
                        continue;
                    }

                    // Non-ambient class + Function: emit TS2813 + TS2814
                    // Note: class & function don't exclude each other in declarations_conflict,
                    // so we handle this case specially with early continue.
                    if (decl_is_class && other_is_function) || (decl_is_function && other_is_class)
                    {
                        let class_idx = if decl_is_class { decl_idx } else { other_idx };
                        if self.is_ambient_class_declaration(class_idx) {
                            continue;
                        }
                        // Non-ambient class + function detected — mark both for TS2813/TS2814
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                        continue;
                    }

                    // In merged namespaces, classes with the same name in different
                    // namespace blocks don't conflict (one exported, one local).
                    if decl_is_class && other_is_class {
                        let decl_ns = self.get_enclosing_namespace(decl_idx);
                        let other_ns = self.get_enclosing_namespace(other_idx);
                        // Both inside namespaces, but different namespace declaration blocks
                        if decl_ns != NodeIndex::NONE
                            && other_ns != NodeIndex::NONE
                            && decl_ns != other_ns
                        {
                            continue;
                        }
                    }

                    // Skip conflict between declarations in different block scopes.
                    // The binder may merge declarations into the same symbol even when they're
                    // in different scopes (e.g., var+let in switch blocks, let in separate blocks).
                    // Check if declarations share the same enclosing block scope.
                    let decl_is_var = (decl_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0;
                    let other_is_var = (other_flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0;
                    let decl_is_block = (decl_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0;
                    let other_is_block = (other_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0;

                    // var + let/const: check if they're in different scopes
                    if (decl_is_var && other_is_block) || (decl_is_block && other_is_var) {
                        let block_idx = if decl_is_block { decl_idx } else { other_idx };
                        let block_scope = self.get_enclosing_block_scope(block_idx);
                        // If the block-scoped variable is inside any block scope,
                        // it's in a nested scope relative to the var
                        if block_scope != NodeIndex::NONE {
                            continue;
                        }
                    }
                    // let/const + let/const: check if they share the same block scope
                    if decl_is_block && other_is_block {
                        let decl_scope = self.get_enclosing_block_scope(decl_idx);
                        let other_scope = self.get_enclosing_block_scope(other_idx);
                        if decl_scope != other_scope {
                            continue;
                        }
                    }

                    // Two exported `var` declarations with the same name conflict (TS2323).
                    // Regular `var` redeclarations are legal in JS, but exported vars
                    // create ambiguity in module export bindings.
                    if (decl_is_var
                        && other_is_var
                        && self.is_exported_variable_declaration(decl_idx)
                        && self.is_exported_variable_declaration(other_idx))
                        || Self::declarations_conflict(decl_flags, other_flags)
                    {
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                    }
                }
            }

            for &namespace_idx in &namespace_order_errors {
                let error_node = self
                    .get_declaration_name_node(namespace_idx)
                    .unwrap_or(namespace_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                    diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                );
            }

            if conflicts.is_empty() {
                continue;
            }

            // Handle TS2393: Duplicate function implementation.
            // When 2+ function declarations with bodies share a name, emit TS2393 on each.
            // This runs BEFORE TS2813/TS2814 handling since that removes function indices.
            {
                let func_impls_with_scope: Vec<(NodeIndex, NodeIndex)> = declarations
                    .iter()
                    .filter(|(decl_idx, flags)| {
                        conflicts.contains(decl_idx)
                            && (flags & symbol_flags::FUNCTION) != 0
                            && self.function_has_body(*decl_idx)
                    })
                    .map(|(idx, _)| (*idx, self.get_enclosing_block_scope(*idx)))
                    .collect();

                // Group by block scope - only functions in the SAME scope are duplicates.
                // Functions in different block scopes (e.g., if/else branches) shadow
                // rather than conflict, so they should not emit TS2393.
                let mut scope_groups: std::collections::HashMap<NodeIndex, Vec<NodeIndex>> =
                    std::collections::HashMap::new();
                for &(idx, scope) in &func_impls_with_scope {
                    scope_groups.entry(scope).or_default().push(idx);
                }

                for group in scope_groups.values() {
                    if group.len() > 1 {
                        for &idx in group {
                            let error_node = self.get_declaration_name_node(idx).unwrap_or(idx);
                            self.error_at_node(
                                error_node,
                                diagnostic_messages::DUPLICATE_FUNCTION_IMPLEMENTATION,
                                diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                            );
                            conflicts.remove(&idx);
                        }
                    }
                }
                // Also remove all function impls from conflicts since they were handled
                for &(idx, _) in &func_impls_with_scope {
                    conflicts.remove(&idx);
                }
                if conflicts.is_empty() {
                    continue;
                }
            }

            // Check for class + function conflicts (TS2813 + TS2814)
            // These get special diagnostics instead of the generic TS2300
            let has_class_function_conflict = {
                let has_class = declarations.iter().any(|(decl_idx, flags)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::CLASS) != 0
                });
                let has_function = declarations.iter().any(|(decl_idx, flags)| {
                    conflicts.contains(decl_idx) && (flags & symbol_flags::FUNCTION) != 0
                });
                has_class && has_function
            };

            if has_class_function_conflict {
                let name = symbol.escaped_name.clone();

                // Emit TS2813 on class declarations
                for &(decl_idx, flags) in &declarations {
                    if conflicts.contains(&decl_idx) && (flags & symbol_flags::CLASS) != 0 {
                        let error_node =
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                        let message = format_message(
                            diagnostic_messages::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                            &[&name],
                        );
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                        );
                    }
                }

                // Emit TS2814 on function declarations
                for &(decl_idx, flags) in &declarations {
                    if conflicts.contains(&decl_idx) && (flags & symbol_flags::FUNCTION) != 0 {
                        let error_node =
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                        self.error_at_node(
                            error_node,
                            diagnostic_messages::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                            diagnostic_codes::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                        );
                    }
                }

                // Remove class/function entries from conflicts so they don't also get TS2300
                let class_function_indices: Vec<NodeIndex> = declarations
                    .iter()
                    .filter(|(decl_idx, flags)| {
                        conflicts.contains(decl_idx)
                            && ((flags & symbol_flags::CLASS) != 0
                                || (flags & symbol_flags::FUNCTION) != 0)
                    })
                    .map(|(idx, _)| *idx)
                    .collect();
                for idx in class_function_indices {
                    conflicts.remove(&idx);
                }

                if conflicts.is_empty() {
                    continue;
                }
            }

            // Check if we have any non-block-scoped declarations (var, function, etc.)
            // Imports (ALIAS) and let/const (BLOCK_SCOPED_VARIABLE) are block-scoped
            let has_non_block_scoped = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && {
                    (flags & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::ALIAS)) == 0
                }
            });

            let name = symbol.escaped_name.clone();

            // Check if any conflicting declaration is an enum
            let has_enum_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx)
                    && (flags & (symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM)) != 0
            });

            let decl_is_exported = |decl_idx: NodeIndex| self.is_declaration_exported(decl_idx);

            let has_variable_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && (flags & symbol_flags::VARIABLE) != 0
            });
            let has_non_variable_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && (flags & symbol_flags::VARIABLE) == 0
            });
            let has_exported_variable_conflict = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx)
                    && (flags & symbol_flags::VARIABLE) != 0
                    && decl_is_exported(*decl_idx)
            });

            let (message, code) = if has_exported_variable_conflict
                && has_variable_conflict
                && !has_non_variable_conflict
            {
                (
                    format_message(
                        diagnostic_messages::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                        &[&name],
                    ),
                    diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                )
            } else if has_enum_conflict && has_non_block_scoped {
                // Enum merging conflict: TS2567
                (
                    diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS
                        .to_string(),
                    diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                )
            } else if !has_non_block_scoped {
                // Pure block-scoped duplicates (let/const/import conflicts) emit TS2451
                (
                    format_message(
                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                        &[&name],
                    ),
                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                )
            } else {
                // Mixed or non-block-scoped duplicates emit TS2300
                // When TS2395 already fired for this symbol, skip TS2300 — the root cause
                // is export visibility mismatch, not a true duplicate name.
                if has_ts2395 {
                    continue;
                }
                (
                    format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]),
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                )
            };
            for (decl_idx, _) in declarations {
                if conflicts.contains(&decl_idx) {
                    let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    self.error_at_node(error_node, &message, code);
                }
            }
        }
    }

    /// Check if a function declaration has a body (is an implementation, not just a signature).
    pub(crate) fn function_has_body(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        !func.body.is_none()
    }

    /// Get the `NodeIndex` of the nearest enclosing `MODULE_DECLARATION` (namespace) for a declaration.
    /// Returns `NodeIndex::NONE` if the declaration is not inside a namespace.
    fn get_enclosing_namespace(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return parent;
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return NodeIndex::NONE;
            }
            current = parent;
        }
    }

    /// Get the `NodeIndex` of the nearest enclosing block scope for a declaration.
    /// Returns the first Block, `CaseBlock`, `ForStatement`, etc. ancestor.
    /// Returns `NodeIndex::NONE` if the declaration is directly in a function/module scope.
    fn get_enclosing_block_scope(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            match parent_node.kind {
                // Block-creating scopes - return this as the enclosing scope
                syntax_kind_ext::BLOCK
                | syntax_kind_ext::CASE_BLOCK
                | syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT => {
                    return parent;
                }
                // Function/module boundaries - no enclosing block scope
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::SOURCE_FILE => {
                    return NodeIndex::NONE;
                }
                _ => {}
            }
            current = parent;
        }
    }

    /// Check diagnostics specific to merged enum declarations.
    ///
    /// - TS2432: In an enum with multiple declarations, only one declaration can
    ///   omit an initializer for its first enum element.
    /// - TS2300: Duplicate enum member names across different enum declarations.
    fn check_merged_enum_declaration_diagnostics(&mut self, declarations: &[(NodeIndex, u32)]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        let enum_declarations: Vec<NodeIndex> = declarations
            .iter()
            .filter(|&(_decl_idx, flags)| (flags & symbol_flags::ENUM) != 0)
            .map(|(decl_idx, _flags)| *decl_idx)
            .collect();

        if enum_declarations.len() <= 1 {
            return;
        }

        let mut first_member_without_initializer = Vec::new();
        let mut first_decl_for_member_by_name: FxHashMap<String, NodeIndex> = FxHashMap::default();

        for &enum_decl_idx in &enum_declarations {
            let Some(enum_decl_node) = self.ctx.arena.get(enum_decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(enum_decl_node) else {
                continue;
            };

            if let Some(&first_member_idx) = enum_decl.members.nodes.first()
                && let Some(first_member_node) = self.ctx.arena.get(first_member_idx)
                && let Some(first_member) = self.ctx.arena.get_enum_member(first_member_node)
                && first_member.initializer.is_none()
            {
                first_member_without_initializer.push(first_member_idx);
            }

            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(member_name_node) = self.ctx.arena.get(member.name) else {
                    continue;
                };

                let member_name =
                    if let Some(ident) = self.ctx.arena.get_identifier(member_name_node) {
                        ident.escaped_text.clone()
                    } else if let Some(literal) = self.ctx.arena.get_literal(member_name_node) {
                        literal.text.clone()
                    } else {
                        continue;
                    };

                if let Some(&first_decl_idx) = first_decl_for_member_by_name.get(&member_name) {
                    if first_decl_idx != enum_decl_idx {
                        self.error_at_node_msg(
                            member.name,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                            &[&member_name],
                        );
                    }
                } else {
                    first_decl_for_member_by_name.insert(member_name.clone(), enum_decl_idx);
                }
            }
        }

        if first_member_without_initializer.len() > 1 {
            // The first declaration that omits an initializer is allowed;
            // only subsequent ones get TS2432.
            for &member_idx in &first_member_without_initializer[1..] {
                self.error_at_node_msg(
                    member_idx,
                    diagnostic_codes::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                    &[],
                );
            }
        }
    }

    /// Check diagnostics specific to merged interface declarations.
    ///
    /// - TS2717: Subsequent property declarations with the same name must have identical types.
    /// - TS2413: Merged index signatures must be compatible.
    fn check_merged_interface_declaration_diagnostics(&mut self, declarations: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        if declarations.len() <= 1 {
            return;
        }

        let mut declarations_by_scope: FxHashMap<NodeIndex, Vec<NodeIndex>> = FxHashMap::default();
        for &decl_idx in declarations {
            let scope = self.get_enclosing_namespace(decl_idx);
            declarations_by_scope
                .entry(scope)
                .or_default()
                .push(decl_idx);
        }

        for (_, mut declarations_in_scope) in declarations_by_scope {
            if declarations_in_scope.len() <= 1 {
                continue;
            }

            // Merge diagnostics only when interface type parameters are identical.
            // TS2428 is reported separately; once mismatched, compatibility checks
            // should not be compared across declarations in the same scope.
            let Some(first_decl) = declarations_in_scope.first().copied() else {
                continue;
            };
            if !declarations_in_scope[1..].iter().all(|&decl_idx| {
                self.interface_type_parameters_are_merge_compatible(first_decl, decl_idx)
            }) {
                continue;
            }

            declarations_in_scope.sort_by_key(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .map(|node| node.pos)
                    .unwrap_or(u32::MAX)
            });

            let mut merged_string_index: Option<TypeId> = None;
            let mut merged_number_index: Option<TypeId> = None;
            let mut merged_properties: FxHashMap<String, TypeId> = FxHashMap::default();

            for &decl_idx in &declarations_in_scope {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(iface) = self.ctx.arena.get_interface(node) else {
                    continue;
                };

                // Resolve interface-local type parameters before reading member signatures.
                let (_type_params, updates) = self.push_type_parameters(&iface.type_parameters);

                let mut local_properties: Vec<(String, NodeIndex, TypeId, bool)> = Vec::new();
                let mut local_string_index: Option<TypeId> = None;
                let mut local_number_index: Option<TypeId> = None;
                let mut local_string_index_node = NodeIndex::NONE;
                let mut local_number_index_node = NodeIndex::NONE;

                for &member_idx in &iface.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };

                    if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
                        let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(sig.name) else {
                            continue;
                        };

                        let is_numeric_name = self
                            .ctx
                            .arena
                            .get(sig.name)
                            .is_some_and(|n| n.kind == SyntaxKind::NumericLiteral as u16);
                        let property_type = if sig.type_annotation.is_none() {
                            TypeId::ANY
                        } else {
                            self.get_type_from_type_node(sig.type_annotation)
                        };
                        local_properties.push((name, sig.name, property_type, is_numeric_name));
                    } else if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                        let Some(index_sig) = self.ctx.arena.get_index_signature(member_node)
                        else {
                            continue;
                        };
                        let Some(param_idx) = index_sig.parameters.nodes.first().copied() else {
                            continue;
                        };
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if param.type_annotation.is_none() {
                            continue;
                        }
                        let key_type = self.get_type_from_type_node(param.type_annotation);
                        let value_type = if index_sig.type_annotation.is_none() {
                            continue;
                        } else {
                            self.get_type_from_type_node(index_sig.type_annotation)
                        };
                        if self.type_contains_error(key_type)
                            || self.type_contains_error(value_type)
                        {
                            continue;
                        }

                        if key_type == TypeId::STRING {
                            local_string_index = Some(value_type);
                            local_string_index_node = member_idx;
                        } else if key_type == TypeId::NUMBER {
                            local_number_index = Some(value_type);
                            local_number_index_node = member_idx;
                        }
                    }
                }

                // Apply merged declarations checks for property signatures.
                for (name, name_idx, property_type, is_numeric) in &local_properties {
                    if let Some(existing_type) = merged_properties.get(name) {
                        if self.type_contains_error(*property_type)
                            || self.type_contains_error(*existing_type)
                        {
                            continue;
                        }

                        let compatible_both_ways = self
                            .is_assignable_to(*existing_type, *property_type)
                            && self.is_assignable_to(*property_type, *existing_type);
                        if !compatible_both_ways {
                            let existing_type_str = self.format_type(*existing_type);
                            let property_type_str = self.format_type(*property_type);
                            self.error_at_node_msg(
                                *name_idx,
                                diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                &[name, &existing_type_str, &property_type_str],
                            );
                        }
                    } else {
                        // Keep first declaration as canonical for subsequent comparisons.
                        // Matching declarations are not yet merged into this map.
                    }

                    if *is_numeric
                        && let Some(number_index) = local_number_index.or(merged_number_index)
                        && !self.is_assignable_to(*property_type, number_index)
                    {
                        let index_type_str = self.format_type(number_index);
                        self.error_at_node_msg(
                            *name_idx,
                            diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &[
                                name,
                                &self.format_type(*property_type),
                                "number",
                                &index_type_str,
                            ],
                        );
                    }

                    if let Some(string_index) = local_string_index.or(merged_string_index)
                        && !self.is_assignable_to(*property_type, string_index)
                    {
                        let index_type_str = self.format_type(string_index);
                        self.error_at_node_msg(
                            *name_idx,
                            diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &[
                                name,
                                &self.format_type(*property_type),
                                "string",
                                &index_type_str,
                            ],
                        );
                    }
                }

                for (name, _name_idx, property_type, _is_numeric) in local_properties {
                    merged_properties.entry(name).or_insert(property_type);
                }

                // Check declaration-local index signatures against already-seen signatures.
                if let Some(local_number) = local_number_index {
                    if let Some(existing_string) = merged_string_index {
                        let number_str = self.format_type(local_number);
                        let string_str = self.format_type(existing_string);
                        if !self.is_assignable_to(local_number, existing_string) {
                            self.error_at_node_msg(
                                local_number_index_node,
                                diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                                &["number", &number_str, "string", &string_str],
                            );
                        }
                    }

                    if let Some(existing_number) = merged_number_index {
                        let local_str = self.format_type(local_number);
                        let existing_str = self.format_type(existing_number);
                        if !self.is_assignable_to(local_number, existing_number)
                            && !self.is_assignable_to(existing_number, local_number)
                        {
                            self.error_at_node_msg(
                                local_number_index_node,
                                diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                                &["number", &local_str, "number", &existing_str],
                            );
                        }
                    }
                }

                if let Some(local_string) = local_string_index {
                    if let Some(existing_string) = merged_string_index {
                        let local_str = self.format_type(local_string);
                        let existing_str = self.format_type(existing_string);
                        if !self.is_assignable_to(local_string, existing_string)
                            && !self.is_assignable_to(existing_string, local_string)
                        {
                            self.error_at_node_msg(
                                local_string_index_node,
                                diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                                &["string", &local_str, "string", &existing_str],
                            );
                        }
                    }

                    if let Some(existing_number) = merged_number_index {
                        let string_str = self.format_type(local_string);
                        let existing_str = self.format_type(existing_number);
                        if !self.is_assignable_to(existing_number, local_string) {
                            self.error_at_node_msg(
                                local_string_index_node,
                                diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                                &["number", &existing_str, "string", &string_str],
                            );
                        }
                    }
                }

                if merged_number_index.is_none()
                    && let Some(local_number) = local_number_index
                {
                    merged_number_index = Some(local_number);
                }

                if merged_string_index.is_none()
                    && let Some(local_string) = local_string_index
                {
                    merged_string_index = Some(local_string);
                }

                self.pop_type_parameters(updates);
            }
        }
    }

    /// Check for unused declarations (TS6133, TS6192).
    /// Reports variables, functions, classes, and other declarations that are never referenced.
    /// Also reports import declarations where ALL imports are unused (TS6192).
    pub(crate) fn check_unused_declarations(&mut self) {
        use crate::diagnostics::Diagnostic;
        use std::collections::{HashMap, HashSet};
        use tsz_binder::ContainerKind;
        use tsz_binder::symbol_flags;

        let check_locals = self.ctx.no_unused_locals();
        let check_params = self.ctx.no_unused_parameters();
        let is_module = self.ctx.binder.is_external_module();

        // Skip .d.ts files entirely (ambient declarations)
        if self.ctx.file_name.ends_with(".d.ts") {
            return;
        }

        // Collect symbols from scopes.
        // For script files (non-module), skip the root SourceFile scope since
        // top-level declarations are globals and not checked by noUnusedLocals.
        // For module files, check all scopes including root.
        let mut symbols_to_check: Vec<(tsz_binder::SymbolId, String)> = Vec::new();

        for scope in &self.ctx.binder.scopes {
            // Skip root scope in script files
            if !is_module && scope.kind == ContainerKind::SourceFile {
                continue;
            }
            for (name, &sym_id) in scope.table.iter() {
                // Skip lib-originating symbols (e.g. from lib.d.ts)
                if self.ctx.binder.lib_symbol_ids.contains(&sym_id) {
                    continue;
                }
                symbols_to_check.push((sym_id, name.clone()));
            }
        }

        let file_name = self.ctx.file_name.clone();

        // Track import declarations for TS6192.
        // Map from import declaration NodeIndex to (total_count, unused_count).
        let mut import_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track variable declarations for TS6199.
        // Map from variable declaration NodeIndex to (total_count, unused_count).
        let mut variable_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track destructuring patterns for TS6198.
        // Map from binding pattern NodeIndex to (total_elements, unused_elements).
        let destructuring_patterns: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // First pass: identify ALL import symbols and track them by import declaration.
        // This includes both used and unused imports.
        for (_sym_id, _name) in &symbols_to_check {
            let sym_id = *_sym_id;
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let flags = symbol.flags;

            // Only track ALIAS symbols (imports)
            if (flags & symbol_flags::ALIAS) == 0 {
                continue;
            }

            // Get the declaration node
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            // Find the parent IMPORT_DECLARATION node
            if let Some(import_decl_idx) = self.find_parent_import_declaration(decl_idx) {
                let is_used = self.ctx.referenced_symbols.borrow().contains(&sym_id);
                let entry = import_declarations.entry(import_decl_idx).or_insert((0, 0));
                entry.0 += 1; // total count
                if !is_used {
                    entry.1 += 1; // unused count
                }
            }
        }

        // Second pass: track variable declarations (for TS6199)
        // We need to track VARIABLE_DECLARATION nodes (not individual variables)
        // to distinguish `var x, y;` (2 decls) from `const {a, b} = obj;` (1 decl with multiple bindings)
        let mut var_decl_list_children: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
        let mut unused_var_decls: HashSet<NodeIndex> = HashSet::new();
        let mut pattern_children: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
        let mut unused_pattern_elements: HashSet<NodeIndex> = HashSet::new();

        for (_sym_id, _name) in &symbols_to_check {
            let sym_id = *_sym_id;
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let flags = symbol.flags;

            // Only track variables (not imports, not parameters)
            let is_var = (flags
                & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::FUNCTION_SCOPED_VARIABLE))
                != 0;
            if !is_var {
                continue;
            }

            // Get the declaration node
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            // Skip if this is a parameter
            if self.is_parameter_declaration(decl_idx) {
                continue;
            }

            // Find the parent VARIABLE_DECLARATION and VARIABLE_DECLARATION_LIST
            if let Some(var_decl_node_idx) = self.find_parent_variable_decl_node(decl_idx)
                && let Some(var_decl_list_idx) =
                    self.find_parent_variable_declaration(var_decl_node_idx)
            {
                // Track this VARIABLE_DECLARATION node under its parent list
                var_decl_list_children
                    .entry(var_decl_list_idx)
                    .or_default()
                    .insert(var_decl_node_idx);

                // Check if this variable is unused
                let is_used = self.ctx.referenced_symbols.borrow().contains(&sym_id);
                if !is_used {
                    unused_var_decls.insert(var_decl_node_idx);
                }

                if let Some(pattern_idx) = self.find_parent_binding_pattern(decl_idx) {
                    pattern_children
                        .entry(pattern_idx)
                        .or_default()
                        .insert(decl_idx);
                    if !is_used {
                        unused_pattern_elements.insert(decl_idx);
                    }
                }
            }
        }

        // Now count VARIABLE_DECLARATION nodes (not variables) in each list
        for (var_decl_list_idx, decl_nodes) in &var_decl_list_children {
            let total_count = decl_nodes.len();
            let unused_count = decl_nodes
                .iter()
                .filter(|n| unused_var_decls.contains(n))
                .count();
            variable_declarations.insert(*var_decl_list_idx, (total_count, unused_count));
        }

        for (sym_id, name) in symbols_to_check {
            // Skip if already referenced
            if self.ctx.referenced_symbols.borrow().contains(&sym_id) {
                continue;
            }

            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            let flags = symbol.flags;

            // Skip exported symbols — they may be used externally
            if symbol.is_exported || (flags & symbol_flags::EXPORT_VALUE) != 0 {
                continue;
            }

            // Skip special/internal names
            if name == "default" || name == "__export" || name == "arguments" || name == "React"
            // JSX factory — always considered used when JSX is enabled
            {
                continue;
            }

            // Skip type parameters — they are handled separately (not in binder scope)
            if (flags & symbol_flags::TYPE_PARAMETER) != 0 {
                continue;
            }

            // Skip non-private members (constructors, signatures, enum members, prototype)
            // Private members ARE checked under noUnusedLocals (TS6133)
            let is_member = (flags
                & (symbol_flags::PROPERTY
                    | symbol_flags::METHOD
                    | symbol_flags::GET_ACCESSOR
                    | symbol_flags::SET_ACCESSOR))
                != 0;
            if is_member {
                // Only private members get unused checking — use PRIVATE flag set by binder
                let is_private = (flags & symbol_flags::PRIVATE) != 0;
                if !is_private {
                    continue; // Public/protected members may be used externally
                }
                // Setter-only private members are "used" by write accesses.
                // TSC never flags them as unused since writes count as usage.
                let is_setter_only = (flags & symbol_flags::SET_ACCESSOR) != 0
                    && (flags & symbol_flags::GET_ACCESSOR) == 0;
                if is_setter_only {
                    continue;
                }
                // Fall through to check private members
            }

            // Always skip constructors, signatures, enum members, prototype
            if (flags
                & (symbol_flags::CONSTRUCTOR
                    | symbol_flags::SIGNATURE
                    | symbol_flags::ENUM_MEMBER
                    | symbol_flags::PROTOTYPE))
                != 0
            {
                continue;
            }

            // Get the declaration node for position info
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // Ambient declarations (`declare ...` or declarations nested in ambient
            // contexts) are not checked by noUnusedLocals.
            if self.is_ambient_declaration(decl_idx) {
                continue;
            }

            // Skip catch clause variables — TSC exempts them from unused checking
            if self.is_catch_clause_variable(decl_idx) {
                continue;
            }

            // Skip using/await using declarations — they always have dispose side effects
            if self.is_using_declaration(decl_idx) {
                continue;
            }

            // Skip named function expression names — TSC never flags these as unused.
            // `var x = function somefn() {}` binds `somefn` in its own scope.
            if decl_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                && (flags & symbol_flags::FUNCTION) != 0
            {
                continue;
            }

            // Determine what kind of symbol this is and whether we should check it
            if check_locals {
                // Check local variables, functions, classes, interfaces, type aliases, imports
                let is_checkable_local = (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                    || (flags & symbol_flags::FUNCTION) != 0
                    || (flags & symbol_flags::CLASS) != 0
                    || (flags & symbol_flags::INTERFACE) != 0
                    || (flags & symbol_flags::TYPE_ALIAS) != 0
                    || (flags & symbol_flags::ALIAS) != 0  // imports
                    || (flags & symbol_flags::REGULAR_ENUM) != 0
                    || (flags & symbol_flags::CONST_ENUM) != 0;

                // var declarations that aren't parameters
                let is_var = (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                    && !self.is_parameter_declaration(decl_idx);

                // Private class members (property, method, accessor)
                let is_private_member = is_member;

                // Non-exported namespaces/modules
                let is_unused_namespace =
                    (flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE)) != 0;

                if is_checkable_local || is_var || is_private_member || is_unused_namespace {
                    // For imports, check if this is part of an import declaration where ALL imports are unused.
                    // If so, skip emitting TS6133 here because TS6192 will be emitted for the entire declaration.
                    // Only skip when there are MULTIPLE imports (single unused imports get TS6133).
                    let is_import = (flags & symbol_flags::ALIAS) != 0;
                    let skip_import_ts6133 = if is_import {
                        if let Some(import_decl_idx) = self.find_parent_import_declaration(decl_idx)
                        {
                            if let Some(&(total_count, unused_count)) =
                                import_declarations.get(&import_decl_idx)
                            {
                                // Skip TS6133 only if there are multiple imports and ALL are unused (TS6192 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // For variables, check if this is part of a variable declaration where ALL variables are unused.
                    // If so, skip emitting TS6133 here because TS6199 will be emitted for the entire declaration.
                    // Only skip when there are MULTIPLE variables (single unused variables get TS6133).
                    let is_variable = (flags
                        & (symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::FUNCTION_SCOPED_VARIABLE))
                        != 0
                        && !self.is_parameter_declaration(decl_idx);
                    let skip_variable_ts6133 = if is_variable {
                        if let Some(var_decl_idx) = self.find_parent_variable_declaration(decl_idx)
                        {
                            if let Some(&(total_count, unused_count)) =
                                variable_declarations.get(&var_decl_idx)
                            {
                                // Skip TS6133 only if there are multiple variables and ALL are unused (TS6199 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // For destructuring patterns, check if this is part of a binding pattern where ALL elements are unused.
                    // If so, skip emitting TS6133 here because TS6198 will be emitted for the entire pattern.
                    // Only skip when there are MULTIPLE elements (single unused elements get TS6133).
                    let skip_destructuring_ts6133 = if is_variable {
                        if let Some(pattern_idx) = self.find_parent_binding_pattern(decl_idx) {
                            if let Some(&(total_count, unused_count)) =
                                destructuring_patterns.get(&pattern_idx)
                            {
                                // Skip TS6133 only if there are multiple elements and ALL are unused (TS6198 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !skip_import_ts6133 && !skip_variable_ts6133 && !skip_destructuring_ts6133 {
                        // Check if write-only (assigned but never read)
                        // Destructured variables should NOT get TS6198 - they get TS6133
                        let is_destructured = self.find_parent_binding_pattern(decl_idx).is_some();
                        let is_write_only =
                            !is_destructured && self.ctx.written_symbols.borrow().contains(&sym_id);

                        // TS6196 for classes, interfaces, type aliases, enums ("never used")
                        // TS6198 for write-only variables ("assigned but never used")
                        // TS6133 for variables, functions, imports, class properties ("value never read")
                        // Note: TS6138 ("Property 'x' is declared but its value is never read")
                        // is only for constructor parameter properties, handled in the parameter section below.
                        let is_type_only = (flags & symbol_flags::CLASS) != 0
                            || (flags & symbol_flags::INTERFACE) != 0
                            || (flags & symbol_flags::TYPE_ALIAS) != 0
                            || (flags & symbol_flags::REGULAR_ENUM) != 0
                            || (flags & symbol_flags::CONST_ENUM) != 0;
                        let (msg, code) = if is_type_only {
                            (format!("'{name}' is declared but never used."), 6196)
                        } else if is_write_only {
                            (
                                format!("'{name}' is assigned a value but never used."),
                                6198,
                            )
                        } else {
                            (
                                format!("'{name}' is declared but its value is never read."),
                                6133,
                            )
                        };
                        let start = decl_node.pos;
                        let length = decl_node.end.saturating_sub(decl_node.pos);
                        self.ctx.push_diagnostic(Diagnostic {
                            file: file_name.clone(),
                            start,
                            length,
                            message_text: msg,
                            category: crate::diagnostics::DiagnosticCategory::Error,
                            code,
                            related_information: Vec::new(),
                        });
                    }
                }
            }

            if check_params {
                // Check function parameters (but not catch clause or overload signature params)
                let is_param = (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                    && self.is_parameter_declaration(decl_idx)
                    && !self.is_overload_signature_parameter(decl_idx);

                // Skip `this` parameter — it's a TypeScript type annotation, not an actual parameter
                if is_param && name == "this" {
                    continue;
                }

                // Skip parameters starting with _ (TSC convention for intentionally unused)
                if is_param && name.starts_with('_') {
                    continue;
                }

                if is_param {
                    let msg = format!("'{name}' is declared but its value is never read.");
                    let start = decl_node.pos;
                    let length = decl_node.end.saturating_sub(decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6133,
                        related_information: Vec::new(),
                    });
                }
            }
        }

        // Emit TS6192 for import declarations where ALL imports are unused.
        // Only emit this when there are MULTIPLE imports (total_count > 1).
        // For single unused imports, TS6133 is emitted above.
        if check_locals {
            for (import_decl_idx, (total_count, unused_count)) in import_declarations {
                // Only emit if there are multiple imports and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(import_decl_node) = self.ctx.arena.get(import_decl_idx)
                {
                    let msg = "All imports in import declaration are unused.".to_string();
                    let start = import_decl_node.pos;
                    let length = import_decl_node.end.saturating_sub(import_decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6192,
                        related_information: Vec::new(),
                    });
                }
            }

            // Emit TS6199 for variable declarations where ALL variables are unused.
            // Only emit this when there are MULTIPLE variables (total_count > 1).
            // For single unused variables, TS6133 is emitted above.
            for (var_decl_idx, (total_count, unused_count)) in variable_declarations {
                // Only emit if there are multiple variables and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx)
                {
                    let msg = "All variables are unused.".to_string();
                    let start = var_decl_node.pos;
                    let length = var_decl_node.end.saturating_sub(var_decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6199,
                        related_information: Vec::new(),
                    });
                }
            }

            // Emit TS6198 for destructuring patterns where ALL elements are unused.
            // Only emit this when there are MULTIPLE elements (total_count > 1).
            // For single unused elements, TS6133 is emitted above.
            for (pattern_idx, (total_count, unused_count)) in destructuring_patterns {
                // Only emit if there are multiple elements and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(pattern_node) = self.ctx.arena.get(pattern_idx)
                {
                    let msg = "All destructured elements are unused.".to_string();
                    let start = pattern_node.pos;
                    let length = pattern_node.end.saturating_sub(pattern_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6198,
                        related_information: Vec::new(),
                    });
                }
            }
        }
    }

    /// Find the parent `IMPORT_DECLARATION` node for an import symbol's declaration.
    fn find_parent_import_declaration(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find IMPORT_DECLARATION
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::IMPORT_DECLARATION
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Find the parent `VARIABLE_DECLARATION` node for a variable symbol's declaration.
    /// This returns the `VARIABLE_DECLARATION` node itself, not the `VARIABLE_DECLARATION_LIST`.
    fn find_parent_variable_decl_node(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find VARIABLE_DECLARATION
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Find the parent `VARIABLE_DECLARATION_LIST` node for a variable symbol's declaration.
    /// This allows us to track all variables declared in a single statement (e.g., `var x, y;`).
    fn find_parent_variable_declaration(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find VARIABLE_DECLARATION_LIST
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Find the parent `BINDING_PATTERN` (`OBJECT_BINDING_PATTERN` or `ARRAY_BINDING_PATTERN`)
    /// for a binding element declaration. This is used to track TS6198 (all destructured
    /// elements are unused).
    fn find_parent_binding_pattern(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find OBJECT_BINDING_PATTERN or ARRAY_BINDING_PATTERN
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Check if a declaration node is a parameter declaration.
    fn is_parameter_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::PARAMETER
    }

    /// Check if a declaration is a `using` or `await using` variable.
    /// These always have dispose side effects, so TSC never flags them as unused.
    fn is_using_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::flags::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        let parent_idx = self
            .ctx
            .arena
            .get_extended(idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);
        if parent_idx.is_none() {
            return false;
        }
        if let Some(parent) = self.ctx.arena.get(parent_idx)
            && parent.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
        {
            let flags = parent.flags as u32;
            if (flags & node_flags::USING) != 0 {
                return true;
            }
        }
        false
    }

    /// Check if a declaration is a catch clause variable.
    fn is_catch_clause_variable(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let parent_idx = self
            .ctx
            .arena
            .get_extended(idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);
        if parent_idx.is_none() {
            return false;
        }
        if let Some(parent) = self.ctx.arena.get(parent_idx)
            && parent.kind == syntax_kind_ext::CATCH_CLAUSE
        {
            return true;
        }
        false
    }

    /// Check if a parameter is in an overload signature (function/method without body).
    /// TSC does not flag parameters in overload signatures as unused.
    fn is_overload_signature_parameter(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        // Walk up from parameter to find containing function/method/constructor
        // Structure: Parameter → SyntaxList/ParameterList → FunctionDecl/MethodDecl/Constructor
        let mut current = idx;
        for _ in 0..5 {
            let parent_idx = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if parent_idx.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent_idx) {
                match parent_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::FUNCTION_EXPRESSION => {
                        if let Some(func) = self.ctx.arena.get_function(parent_node) {
                            return func.body.is_none();
                        }
                        return false;
                    }
                    syntax_kind_ext::METHOD_DECLARATION => {
                        if let Some(method) = self.ctx.arena.get_method_decl(parent_node) {
                            return method.body.is_none();
                        }
                        return false;
                    }
                    syntax_kind_ext::CONSTRUCTOR => {
                        if let Some(ctor) = self.ctx.arena.get_constructor(parent_node) {
                            return ctor.body.is_none();
                        }
                        return false;
                    }
                    _ => {
                        current = parent_idx;
                    }
                }
            } else {
                return false;
            }
        }
        false
    }

    // 23. Import and Private Brand Utilities (moved to symbol_resolver.rs)

    // 25. AST Traversal Utilities (11 functions)

    /// Find the enclosing function-like node for a given node.
    ///
    /// Traverses up the AST to find the first parent that is a function-like
    /// construct (function declaration, function expression, arrow function, method, constructor).
    /// Find if there's a constructor implementation after position `start` in members list.
    ///
    /// ## Parameters
    /// - `members`: Slice of member node indices
    /// - `start`: Position to start searching from
    ///
    /// Returns true if a constructor with a body is found, false otherwise.
    pub(crate) fn find_constructor_impl(&self, members: &[NodeIndex], start: usize) -> bool {
        for member_idx in members.iter().skip(start).copied() {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(ctor) = self.ctx.arena.get_constructor(node)
                    && !ctor.body.is_none()
                {
                    return true;
                }
                // Another constructor overload - keep looking
            } else {
                // Non-constructor member - no implementation found
                return false;
            }
        }
        false
    }

    /// Check if there's a method implementation with the given name after position `start`.
    ///
    /// ## Parameters
    /// - `members`: Slice of member node indices
    /// - `start`: Position to start searching from
    /// - `_name`: The method name to search for
    ///
    /// Returns (found: bool, name: Option<String>).
    pub(crate) fn find_method_impl(
        &self,
        members: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<usize>) {
        for (offset, member_idx) in members.iter().skip(start).copied().enumerate() {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    let member_name = self.get_method_name_from_node(member_idx);
                    if member_name.as_deref() != Some(name) {
                        if method.body.is_some() {
                            // Different name but has body - wrong-named implementation (TS2389)
                            return (true, member_name, Some(start + offset));
                        }
                        // Different name, no body - no implementation found
                        return (false, None, None);
                    }
                    if !method.body.is_none() {
                        // Found the implementation with matching name
                        return (true, member_name, Some(start + offset));
                    }
                    // Same name but no body - another overload signature, keep looking
                }
            } else {
                // Non-method member encountered - no implementation found
                return (false, None, None);
            }
        }
        (false, None, None)
    }

    /// Find a function implementation with the given name after position `start`.
    ///
    /// Recursively searches through statements to find a matching function implementation.
    /// Handles overload signatures by continuing to search through same-name overloads.
    ///
    /// ## Parameters
    /// - `statements`: Slice of statement node indices
    /// - `start`: Position to start searching from
    /// - `name`: The function name to search for
    ///
    /// Returns (found: bool, name: Option<String>, node: Option<NodeIndex>).
    pub(crate) fn find_function_impl(
        &self,
        statements: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<NodeIndex>) {
        if start >= statements.len() {
            return (false, None, None);
        }

        let stmt_idx = statements[start];
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return (false, None, None);
        };

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.ctx.arena.get_function(node)
        {
            // Check if this is an implementation (has body)
            if !func.body.is_none() {
                // This is an implementation - check if name matches
                let impl_name = self.get_function_name_from_node(stmt_idx);
                return (true, impl_name, Some(stmt_idx));
            }

            // Another overload signature without body - need to look further
            // but we should check if this is the same function name
            let overload_name = self.get_function_name_from_node(stmt_idx);
            if overload_name.as_ref() == Some(&name.to_string()) {
                // Same function, continue looking for implementation
                return self.find_function_impl(statements, start + 1, name);
            }
        }

        (false, None, None)
    }
}
