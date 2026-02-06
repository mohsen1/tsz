//! Promise and Async Type Checking Module
//!
//! This module contains Promise and async-related type checking methods
//! extracted from CheckerState as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Promise type detection and validation
//! - Type argument extraction from Promise<T>
//! - Async function return type checking
//! - Promise-like type recognition (Promise, PromiseLike, custom promises)

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver as solver_narrowing;
use tsz_solver::TypeId;
use tsz_solver::type_queries::{PromiseTypeKind, classify_promise_type, get_union_members};

// =============================================================================
// Promise and Async Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Promise Type Detection
    // =========================================================================

    /// Check if a name refers to a Promise-like type.
    ///
    /// Returns true for "Promise", "PromiseLike", or any name containing "Promise".
    /// This handles built-in Promise types as well as custom Promise implementations.
    pub fn is_promise_like_name(&self, name: &str) -> bool {
        matches!(name, "Promise" | "PromiseLike") || name.contains("Promise")
    }

    /// Check if a type reference is a Promise or Promise-like type.
    ///
    /// This handles:
    /// - Direct Promise/PromiseLike references
    /// - Promise<T> type applications
    /// - Object types from lib files (conservatively assumed to be Promise-like)
    pub fn type_ref_is_promise_like(&self, type_id: TypeId) -> bool {
        match classify_promise_type(self.ctx.types, type_id) {
            PromiseTypeKind::Lazy(def_id) => {
                // Phase 4.2: Use DefId -> SymbolId bridge
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        return self.is_promise_like_name(symbol.escaped_name.as_str());
                    }
                }
                false
            }
            PromiseTypeKind::Application { base, .. } => {
                // Check if the base type of the application is a Promise-like type
                self.type_ref_is_promise_like(base)
            }
            PromiseTypeKind::Object(_) => {
                // For Object types (interfaces from lib files), we conservatively assume
                // they might be Promise-like. This avoids false positives for Promise<void>
                // return types from lib files where we can't easily determine the interface name.
                // A more precise check would require tracking the original type reference.
                true
            }
            PromiseTypeKind::Union(_) | PromiseTypeKind::NotPromise => false,
        }
    }

    /// Check if a type is a Promise or Promise-like type.
    ///
    /// This is used to validate async function return types.
    /// Handles both Promise<T> applications and direct Promise references.
    ///
    /// IMPORTANT: This method is STRICT - it only returns true for actual Promise/PromiseLike types.
    /// It does NOT use the conservative assumption that all Object types might be Promise-like.
    /// This ensures TS2705 is correctly emitted for async functions with non-Promise return types.
    pub fn is_promise_type(&self, type_id: TypeId) -> bool {
        match classify_promise_type(self.ctx.types, type_id) {
            PromiseTypeKind::Application { base, .. } => {
                // For Application types, STRICTLY check if the base symbol is Promise/PromiseLike
                // We do NOT use type_ref_is_promise_like here because it conservatively assumes
                // all Object types are Promise-like, which causes false negatives for TS2705
                match classify_promise_type(self.ctx.types, base) {
                    PromiseTypeKind::Lazy(def_id) => {
                        // Phase 4.2: Use DefId -> SymbolId bridge
                        if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                                return self.is_promise_like_name(symbol.escaped_name.as_str());
                            }
                        }
                        false
                    }
                    // Handle nested applications (e.g., Promise<SomeType<T>>)
                    PromiseTypeKind::Application {
                        base: inner_base, ..
                    } => self.is_promise_type(inner_base),
                    _ => false,
                }
            }
            PromiseTypeKind::Lazy(def_id) => {
                // Phase 4.2: Use DefId -> SymbolId bridge
                // Check for direct Promise or PromiseLike reference (this also handles type aliases)
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        return self.is_promise_like_name(symbol.escaped_name.as_str());
                    }
                }
                false
            }
            PromiseTypeKind::Object(_)
            | PromiseTypeKind::Union(_)
            | PromiseTypeKind::NotPromise => false,
        }
    }

    /// Check if the global Promise type is available in lib contexts.
    ///
    /// Used for TS2697: async function must return Promise.
    /// NOTE: Currently unused since TS2697 checks are disabled due to false positives.
    #[allow(dead_code)]
    pub fn is_promise_global_available(&self) -> bool {
        // Use the centralized has_name_in_lib helper which checks:
        // - lib_contexts
        // - current_scope
        // - file_locals
        self.ctx.has_name_in_lib("Promise")
    }

    /// Check if the global Promise type is available, emit TS2318 if not.
    ///
    /// Called when processing async functions to ensure Promise is available.
    /// Matches TSC behavior which emits TS2318 "Cannot find global type 'Promise'"
    /// when the Promise type is not in scope - INCLUDING when noLib is true.
    pub fn check_global_promise_available(&mut self) {
        // Emit TS2318 if Promise is not found, regardless of noLib setting.
        // TSC emits this error even with noLib: true when async functions are used.
        if !self.ctx.has_name_in_lib("Promise") {
            use tsz_binder::lib_loader;
            self.ctx
                .push_diagnostic(lib_loader::emit_error_global_type_missing(
                    "Promise",
                    self.ctx.file_name.clone(),
                    0,
                    0,
                ));
        }
    }

    // =========================================================================
    // Type Argument Extraction
    // =========================================================================

    /// Extract the type argument from a Promise<T> or Promise-like type.
    ///
    /// Returns Some(T) if the type is Promise<T>, None otherwise.
    /// This handles:
    /// - Synthetic PROMISE_BASE type (when Promise symbol wasn't resolved)
    /// - Direct Promise<T> applications
    /// - Type aliases that expand to Promise<T>
    /// - Classes that extend Promise<T>
    pub fn promise_like_return_type_argument(&mut self, return_type: TypeId) -> Option<TypeId> {
        if let PromiseTypeKind::Application { base, args, .. } =
            classify_promise_type(self.ctx.types, return_type)
        {
            // Check for synthetic PROMISE_BASE type (created when Promise symbol wasn't resolved)
            // This allows us to extract T from Promise<T> even without full lib files
            if base == TypeId::PROMISE_BASE {
                if let Some(&first_arg) = args.first() {
                    return Some(first_arg);
                }
            }

            // Try to get the type argument from the base symbol
            if let Some(result) =
                self.promise_like_type_argument_from_base(base, &args, &mut Vec::new())
            {
                return Some(result);
            }

            // Fallback: if the base is a Promise-like reference (e.g., Promise from lib files)
            // and we have type arguments, return the first one
            // This handles cases where Promise doesn't have expected flags or where
            // promise_like_type_argument_from_base fails for other reasons
            match classify_promise_type(self.ctx.types, base) {
                PromiseTypeKind::Lazy(def_id) => {
                    // Phase 4.2: Use DefId -> SymbolId bridge
                    if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            if self.is_promise_like_name(symbol.escaped_name.as_str()) {
                                if let Some(&first_arg) = args.first() {
                                    return Some(first_arg);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // If we can't extract the type argument from a Promise-like type,
        // return None instead of ANY/UNKNOWN (consistent with Task 4-6 changes)
        // This allows the caller (await expressions) to use UNKNOWN as fallback
        None
    }

    /// Extract type argument from a Promise-like base type.
    ///
    /// Handles:
    /// - Direct Promise/PromiseLike types
    /// - Type aliases to Promise types
    /// - Classes that extend Promise
    pub fn promise_like_type_argument_from_base(
        &mut self,
        base: TypeId,
        args: &[TypeId],
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        // Phase 4.2: Handle Lazy variant properly
        let sym_id = match classify_promise_type(self.ctx.types, base) {
            PromiseTypeKind::Lazy(def_id) => {
                // Use DefId -> SymbolId bridge
                self.ctx.def_to_symbol_id(def_id)?
            }
            _ => return None,
        };

        // Try to get the symbol, but handle the case where it doesn't exist (e.g., import from missing module)
        let symbol = self.ctx.binder.get_symbol(sym_id);

        // If symbol doesn't exist, we can still check if we have type arguments to extract
        // This handles cases like `MyPromise<void>` where MyPromise is imported from a missing module
        if symbol.is_none() {
            // For unresolved Promise-like types, assume the inner type is the first type argument
            // This allows async functions with unresolved Promise return types to be handled gracefully
            if let Some(&first_arg) = args.first() {
                return Some(first_arg);
            }
            // Return UNKNOWN instead of ANY when there are no type arguments (consistent with Task 4-6)
            return Some(TypeId::UNKNOWN);
        }

        let symbol = match symbol {
            Some(sym) => sym,
            None => {
                // This should never happen due to the check above, but handle gracefully
                return Some(args.first().copied().unwrap_or(TypeId::UNKNOWN));
            }
        };
        let name = symbol.escaped_name.as_str();

        if self.is_promise_like_name(name) {
            // Return UNKNOWN instead of ANY when there are no type arguments (consistent with Task 4-6)
            return Some(args.first().copied().unwrap_or(TypeId::UNKNOWN));
        }

        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            return self.promise_like_type_argument_from_alias(sym_id, args, visited_aliases);
        }

        if symbol.flags & symbol_flags::CLASS != 0 {
            return self.promise_like_type_argument_from_class(sym_id, args, visited_aliases);
        }

        None
    }

    /// Extract type argument from a type alias that expands to a Promise type.
    ///
    /// For example, given `type MyPromise<T> = Promise<T>`, this extracts
    /// the type argument from MyPromise<U>.
    pub fn promise_like_type_argument_from_alias(
        &mut self,
        sym_id: SymbolId,
        args: &[TypeId],
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        if visited_aliases.contains(&sym_id) {
            return None;
        }
        visited_aliases.push(sym_id);

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return None;
        }

        let node = self.ctx.arena.get(decl_idx)?;
        let type_alias = self.ctx.arena.get_type_alias(node)?;

        let mut bindings = Vec::new();
        if let Some(params) = &type_alias.type_parameters {
            if params.nodes.len() != args.len() {
                return None;
            }
            for (&param_idx, &arg) in params.nodes.iter().zip(args.iter()) {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                bindings.push((self.ctx.types.intern_string(&ident.escaped_text), arg));
            }
        } else if !args.is_empty() {
            return None;
        }

        // Check if the alias RHS is directly a Promise/PromiseLike type reference
        // before lowering (e.g., Promise<T> where Promise is from lib and might not fully resolve)
        if let Some(type_node) = self.ctx.arena.get(type_alias.type_node)
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && self.is_promise_like_name(ident.escaped_text.as_str())
        {
            // It's Promise<...> or PromiseLike<...>
            // Get the first type argument and substitute bindings
            if let Some(type_args) = &type_ref.type_arguments
                && let Some(&first_arg_idx) = type_args.nodes.first()
            {
                // Try to substitute bindings in the type argument
                let arg_type = self.lower_type_with_bindings(first_arg_idx, bindings.clone());
                return Some(arg_type);
            }
            // No type args means Promise (equivalent to Promise<any>)
            return Some(TypeId::ANY);
        }

        let lowered = self.lower_type_with_bindings(type_alias.type_node, bindings);
        if let PromiseTypeKind::Application {
            base: lowered_base,
            args: lowered_args,
            ..
        } = classify_promise_type(self.ctx.types, lowered)
        {
            return self.promise_like_type_argument_from_base(
                lowered_base,
                &lowered_args,
                visited_aliases,
            );
        }

        // Fallback: if the alias expands to a promise-like type reference (e.g., Promise from lib),
        // treat it as Promise<unknown> if we can't get the type argument.
        // This handles cases like: type PromiseAlias<T> = Promise<T> where Promise comes from lib.
        if self.type_ref_is_promise_like(lowered) {
            // If we have args, try to return the first one (the T in Promise<T>)
            // Otherwise return UNKNOWN for stricter type checking
            return Some(args.first().copied().unwrap_or(TypeId::UNKNOWN));
        }

        None
    }

    /// Extract type argument from a class that extends Promise.
    ///
    /// For example, given `class MyPromise<T> extends Promise<T>`, this extracts
    /// the type argument from MyPromise<U>.
    pub fn promise_like_type_argument_from_class(
        &mut self,
        sym_id: SymbolId,
        args: &[TypeId],
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        if visited_aliases.contains(&sym_id) {
            return None;
        }
        visited_aliases.push(sym_id);

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return None;
        }

        let node = self.ctx.arena.get(decl_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        // Build type parameter bindings for this class
        let mut bindings = Vec::new();
        if let Some(params) = &class.type_parameters {
            if params.nodes.len() != args.len() {
                return None;
            }
            for (&param_idx, &arg) in params.nodes.iter().zip(args.iter()) {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                bindings.push((self.ctx.types.intern_string(&ident.escaped_text), arg));
            }
        } else if !args.is_empty() {
            return None;
        }

        // Check heritage clauses for extends Promise/PromiseLike
        let Some(heritage_clauses) = &class.heritage_clauses else {
            return None;
        };

        for &clause_idx in heritage_clauses.nodes.iter() {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;

            // Only check extends clauses (token = ExtendsKeyword = 96)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(type_idx) else {
                continue;
            };

            // Handle both cases:
            // 1. ExpressionWithTypeArguments (e.g., Promise<T>)
            // 2. Simple Identifier (e.g., Promise)
            let (expr_idx, type_arguments) =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    (
                        expr_type_args.expression,
                        expr_type_args.type_arguments.as_ref(),
                    )
                } else {
                    (type_idx, None)
                };

            // Get the base class name
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(expr_node) else {
                continue;
            };

            // Check if it's Promise or PromiseLike
            if !self.is_promise_like_name(&ident.escaped_text) {
                continue;
            }

            // If it extends Promise<X>, extract X and substitute type parameters
            if let Some(type_args) = type_arguments
                && let Some(&first_arg_node) = type_args.nodes.first()
            {
                let lowered = self.lower_type_with_bindings(first_arg_node, bindings);
                return Some(lowered);
            }

            // Promise with no type argument defaults to Promise<any>
            return Some(TypeId::ANY);
        }

        None
    }

    // =========================================================================
    // Return Type Checking for Async Functions
    // =========================================================================

    /// Check if a return type requires a return value.
    ///
    /// Returns false for void, undefined, any, never, unknown, error types,
    /// and unions containing void/undefined.
    /// Returns true for all other types.
    pub fn requires_return_value(&self, return_type: TypeId) -> bool {
        // void, undefined, any, never don't require a return value
        if return_type == TypeId::VOID
            || return_type == TypeId::UNDEFINED
            || return_type == TypeId::ANY
            || return_type == TypeId::NEVER
            || return_type == TypeId::UNKNOWN
            || return_type == TypeId::ERROR
        {
            return false;
        }

        // Check for union types that include void/undefined using the solver helper
        if let Some(members) = get_union_members(self.ctx.types, return_type) {
            for member in members.iter() {
                if *member == TypeId::VOID || *member == TypeId::UNDEFINED {
                    return false;
                }
            }
        }

        true
    }

    /// Get the return type for implicit return checking.
    ///
    /// For async functions, this unwraps Promise<T> to get T.
    /// For generator functions, returns UNKNOWN (not fully implemented).
    /// Otherwise, returns the original return type.
    pub fn return_type_for_implicit_return_check(
        &mut self,
        return_type: TypeId,
        is_async: bool,
        is_generator: bool,
    ) -> TypeId {
        if is_generator {
            return TypeId::UNKNOWN; // Generator support not implemented - use UNKNOWN
        }

        if is_async {
            if let Some(inner) = self.promise_like_return_type_argument(return_type) {
                return inner;
            }
        }

        return_type
    }

    /// Check if a return type annotation syntactically looks like Promise<T>.
    ///
    /// This is a fallback for when the type can't be resolved but the syntax is clearly Promise.
    /// Used for better error messages when Promise types are not available.
    pub fn return_type_annotation_looks_like_promise(&self, type_annotation: NodeIndex) -> bool {
        // Get the type node from the annotation
        let Some(node) = self.ctx.arena.get(type_annotation) else {
            return false;
        };

        // Check if it's a type reference with "Promise" name
        if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
            // Get the type name - it could be an identifier or qualified name
            if let Some(name_node) = self.ctx.arena.get(type_ref.type_name) {
                // Check for simple identifier like "Promise"
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    return self.is_promise_like_name(&ident.escaped_text);
                }
                // Also check for qualified names like SomeModule.Promise
                if let Some(qualified) = self.ctx.arena.get_qualified_name(name_node)
                    && let Some(right_node) = self.ctx.arena.get(qualified.right)
                    && let Some(ident) = self.ctx.arena.get_identifier(right_node)
                {
                    return self.is_promise_like_name(&ident.escaped_text);
                }
            }
        }

        false
    }

    /// Check if a type is null or undefined only.
    ///
    /// Returns true for the null type, undefined type, or unions that only
    /// contain null and/or undefined.
    pub fn is_null_or_undefined_only(&self, return_type: TypeId) -> bool {
        solver_narrowing::is_definitely_nullish(self.ctx.types.as_type_database(), return_type)
    }

    // Note: The `lower_type_with_bindings` helper method remains in state.rs
    // as it requires access to private methods `resolve_type_symbol_for_lowering`
    // and `resolve_value_symbol_for_lowering`. This is a deliberate choice to
    // keep the implementation encapsulated while still organizing the promise
    // type checking logic into a separate module.

    // =========================================================================
    // Generator Type Helpers
    // =========================================================================

    /// Extract the TReturn type argument from Generator<Y, R, N> or AsyncGenerator<Y, R, N>.
    ///
    /// For generator functions with explicit return types, the return statement
    /// should be checked against TReturn (the second type argument), not the full
    /// Generator/AsyncGenerator type.
    ///
    /// Returns `Some(TReturn)` if the type is a Generator/AsyncGenerator/Iterator/AsyncIterator
    /// type application with at least 2 type arguments, otherwise `None`.
    pub fn get_generator_return_type_argument(&mut self, type_id: TypeId) -> Option<TypeId> {
        use tsz_solver::type_queries::get_type_application;

        // Check if it's a type application (e.g., Generator<Y, R, N>)
        let app = get_type_application(self.ctx.types, type_id)?;

        // Need at least 2 type arguments (Y and R)
        if app.args.len() < 2 {
            return None;
        }

        // Check if base is Generator, AsyncGenerator, Iterator, or AsyncIterator
        let is_generator_like = self.is_generator_like_base_type(app.base);

        if is_generator_like {
            // Return the second type argument (TReturn)
            Some(app.args[1])
        } else {
            None
        }
    }

    /// Check if a type is a Generator-like base type (Generator, AsyncGenerator,
    /// Iterator, AsyncIterator, IterableIterator, AsyncIterableIterator).
    fn is_generator_like_base_type(&mut self, type_id: TypeId) -> bool {
        use tsz_solver::TypeKey;

        // Fast path: Check for Lazy types to known Generator-like types
        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            if let TypeKey::Lazy(def_id) = type_key {
                // Use def_to_symbol_id to find the symbol
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    if let Some(symbol) = self.get_symbol_globally(sym_id) {
                        if Self::is_generator_like_name(&symbol.escaped_name) {
                            return true;
                        }
                    }
                }
            }
        }

        // Robust check: Resolve the global types and compare TypeIds
        // This handles cases where the type is structural (Object/Callable) rather than a Lazy
        for name in &[
            "Generator",
            "AsyncGenerator",
            "Iterator",
            "AsyncIterator",
            "IterableIterator",
            "AsyncIterableIterator",
        ] {
            // resolve_global_interface_type handles looking up in lib files and merging declarations
            if let Some(global_type) = self.resolve_global_interface_type(name) {
                if global_type == type_id {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a name refers to a Generator-like type.
    fn is_generator_like_name(name: &str) -> bool {
        matches!(
            name,
            "Generator"
                | "AsyncGenerator"
                | "Iterator"
                | "AsyncIterator"
                | "IterableIterator"
                | "AsyncIterableIterator"
        )
    }
}
