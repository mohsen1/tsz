//! Promise/async type checking (detection, type argument extraction, return types).

use crate::query_boundaries::checkers::promise as query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver as solver_narrowing;
use tsz_solver::TypeId;

// =============================================================================
// Promise and Async Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Promise Type Detection
    // =========================================================================

    /// Check if a name refers to a Promise-like type.
    ///
    /// Returns true for "Promise", "`PromiseLike`", or any name containing "Promise".
    /// This handles built-in Promise types as well as custom Promise implementations.
    pub fn is_promise_like_name(&self, name: &str) -> bool {
        matches!(name, "Promise" | "PromiseLike") || name.contains("Promise")
    }

    /// Check if a name refers to exactly the global Promise type (not subclasses).
    ///
    /// TSC's `checkAsyncFunctionReturnType` uses `isReferenceToType(returnType, globalPromiseType)`,
    /// which only accepts the global `Promise` itself — not `PromiseLike`, not subclasses like
    /// `MyPromise extends Promise<T>`, not types merely containing "Promise" in their name.
    fn is_exactly_promise_name(name: &str) -> bool {
        name == "Promise"
    }

    /// Strict check: is this type exactly the global `Promise<T>` type?
    ///
    /// Unlike `is_promise_type` (which broadly matches Promise-like names), this only
    /// returns true for the global `Promise` type itself. Used for TS1064 emission where
    /// TSC requires exactly `Promise<T>`, not subclasses or similarly-named types.
    pub fn is_global_promise_type(&self, type_id: TypeId) -> bool {
        match query::classify_promise_type(self.ctx.types, type_id) {
            query::PromiseTypeKind::Application { base, .. } => {
                match query::classify_promise_type(self.ctx.types, base) {
                    query::PromiseTypeKind::Lazy(def_id) => {
                        if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                        {
                            if Self::is_exactly_promise_name(symbol.escaped_name.as_str()) {
                                return true;
                            }
                            // If the base is a type alias, resolve through it to check
                            // if the alias body references Promise. This handles cases
                            // like `type MyPromise<T> = Promise<T>` where the Application
                            // base is the alias, not the underlying Promise interface.
                            if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
                                return self.type_alias_resolves_to_promise(sym_id, symbol);
                            }
                        }
                        false
                    }
                    query::PromiseTypeKind::TypeQuery(sym_ref) => {
                        let sym_id = SymbolId(sym_ref.0);
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            return Self::is_exactly_promise_name(symbol.escaped_name.as_str());
                        }
                        false
                    }
                    query::PromiseTypeKind::Application {
                        base: inner_base, ..
                    } => self.is_global_promise_type(inner_base),
                    _ => false,
                }
            }
            query::PromiseTypeKind::Lazy(def_id) => {
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    return Self::is_exactly_promise_name(symbol.escaped_name.as_str());
                }
                false
            }
            query::PromiseTypeKind::TypeQuery(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    return Self::is_exactly_promise_name(symbol.escaped_name.as_str());
                }
                false
            }
            query::PromiseTypeKind::Object(_)
            | query::PromiseTypeKind::Union(_)
            | query::PromiseTypeKind::NotPromise => false,
        }
    }

    /// Check if a type reference is a Promise or Promise-like type.
    ///
    /// This handles:
    /// - Direct Promise/PromiseLike references
    /// - Promise<T> type applications
    /// - Object types from lib files (conservatively assumed to be Promise-like)
    pub fn type_ref_is_promise_like(&self, type_id: TypeId) -> bool {
        match query::classify_promise_type(self.ctx.types, type_id) {
            query::PromiseTypeKind::Lazy(def_id) => {
                // Use DefId -> SymbolId bridge
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    return self.is_promise_like_name(symbol.escaped_name.as_str());
                }
                false
            }
            query::PromiseTypeKind::TypeQuery(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    return self.is_promise_like_name(symbol.escaped_name.as_str());
                }
                false
            }
            query::PromiseTypeKind::Application { base, .. } => {
                // Check if the base type of the application is a Promise-like type
                self.type_ref_is_promise_like(base)
            }
            query::PromiseTypeKind::Object(_) => {
                // For Object types (interfaces from lib files), we conservatively assume
                // they might be Promise-like. This avoids false positives for Promise<void>
                // return types from lib files where we can't easily determine the interface name.
                // A more precise check would require tracking the original type reference.
                true
            }
            query::PromiseTypeKind::Union(_) | query::PromiseTypeKind::NotPromise => false,
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
        match query::classify_promise_type(self.ctx.types, type_id) {
            query::PromiseTypeKind::Application { base, .. } => {
                // For Application types, STRICTLY check if the base symbol is Promise/PromiseLike
                // We do NOT use type_ref_is_promise_like here because it conservatively assumes
                // all Object types are Promise-like, which causes false negatives for TS2705
                match query::classify_promise_type(self.ctx.types, base) {
                    query::PromiseTypeKind::Lazy(def_id) => {
                        // Use DefId -> SymbolId bridge
                        if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                        {
                            return self.is_promise_like_name(symbol.escaped_name.as_str());
                        }
                        false
                    }
                    query::PromiseTypeKind::TypeQuery(sym_ref) => {
                        let sym_id = SymbolId(sym_ref.0);
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            return self.is_promise_like_name(symbol.escaped_name.as_str());
                        }
                        false
                    }
                    // Handle nested applications (e.g., Promise<SomeType<T>>)
                    query::PromiseTypeKind::Application {
                        base: inner_base, ..
                    } => self.is_promise_type(inner_base),
                    _ => false,
                }
            }
            query::PromiseTypeKind::Lazy(def_id) => {
                // Use DefId -> SymbolId bridge
                // Check for direct Promise or PromiseLike reference (this also handles type aliases)
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    return self.is_promise_like_name(symbol.escaped_name.as_str());
                }
                false
            }
            query::PromiseTypeKind::TypeQuery(sym_ref) => {
                let sym_id = SymbolId(sym_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    return self.is_promise_like_name(symbol.escaped_name.as_str());
                }
                false
            }
            query::PromiseTypeKind::Object(_)
            | query::PromiseTypeKind::Union(_)
            | query::PromiseTypeKind::NotPromise => false,
        }
    }

    /// Extract a Promise member from a contextual type that may be a union.
    ///
    /// When the contextual type for a `new Promise(...)` expression is a union like
    /// `void | PromiseLike<void> | Promise<void>` (as constructed for async function
    /// return expressions), this method finds and returns the `Promise<T>` member.
    /// If the type is already a direct Promise type, returns it as-is.
    /// Returns `None` if no Promise member is found.
    pub fn find_promise_in_contextual_type(&self, type_id: TypeId) -> Option<TypeId> {
        // Fast path: type is already a Promise type
        if self.is_promise_type(type_id) {
            return Some(type_id);
        }

        // Check union members for a Promise type
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            for member in &members {
                if self.is_promise_type(*member) {
                    return Some(*member);
                }
            }
        }

        None
    }

    /// Check if the global Promise type is available, emit TS2318 if not.
    ///
    /// Called when processing async functions to ensure Promise is available.
    /// Matches TSC behavior which emits TS2318 "Cannot find global type 'Promise'"
    /// when the Promise type is not in scope - INCLUDING when noLib is true.
    ///
    /// Routes through the environment capability boundary for the decision.
    pub fn check_global_promise_available(&mut self) {
        // Use the capability boundary to determine if Promise is required and missing.
        // The boundary's check_feature_gate(AsyncFunction) checks lib availability;
        // we additionally verify the type is actually absent from loaded libs.
        if !self.ctx.has_name_in_lib("Promise") {
            let file_name = self.ctx.file_name.clone();
            self.error_global_type_missing_at_position("Promise", file_name, 0, 0);
        }
    }

    // =========================================================================
    // Type Argument Extraction
    // =========================================================================

    /// Extract the type argument from a Promise<T> or Promise-like type.
    ///
    /// Returns Some(T) if the type is Promise<T>, None otherwise.
    /// This handles:
    /// - Synthetic `PROMISE_BASE` type (when Promise symbol wasn't resolved)
    /// - Direct Promise<T> applications
    /// - Type aliases that expand to Promise<T>
    /// - Classes that extend Promise<T>
    pub fn promise_like_return_type_argument(&mut self, return_type: TypeId) -> Option<TypeId> {
        if let query::PromiseTypeKind::Application { base, args, .. } =
            query::classify_promise_type(self.ctx.types, return_type)
        {
            let first_arg = args.first().copied();

            // Check for synthetic PROMISE_BASE type (created when Promise symbol wasn't resolved)
            // This allows us to extract T from Promise<T> even without full lib files
            if base == TypeId::PROMISE_BASE
                && let Some(first_arg) = first_arg
            {
                return Some(first_arg);
            }

            // Fast path: direct Promise/PromiseLike application from lib symbols.
            // This is a hot path for `await Promise.resolve(...)` and avoids
            // heavier alias/class resolution when the base already names Promise.
            if let query::PromiseTypeKind::Lazy(def_id) =
                query::classify_promise_type(self.ctx.types, base)
                && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && self.is_promise_like_name(symbol.escaped_name.as_str())
            {
                return Some(first_arg.unwrap_or(TypeId::UNKNOWN));
            }

            // Handle TypeQuery(SymbolRef) base — the return type annotation stores
            // Promise<T> as Application(TypeQuery(Promise_SymbolRef), [T]) when the
            // base reference is a `typeof` value symbol rather than a Lazy(DefId).
            if let query::PromiseTypeKind::TypeQuery(sym_ref) =
                query::classify_promise_type(self.ctx.types, base)
            {
                let sym_id = SymbolId(sym_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && self.is_promise_like_name(symbol.escaped_name.as_str())
                {
                    return Some(first_arg.unwrap_or(TypeId::UNKNOWN));
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
            if let query::PromiseTypeKind::Lazy(def_id) =
                query::classify_promise_type(self.ctx.types, base)
            {
                // Use DefId -> SymbolId bridge
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && self.is_promise_like_name(symbol.escaped_name.as_str())
                {
                    return Some(first_arg.unwrap_or(TypeId::UNKNOWN));
                }
            }
        }

        // Handle Object shapes: when a Promise<T> type annotation gets evaluated to
        // an Object shape, the Application wrapper is lost. We can still extract T
        // by looking at the `then` method's `onfulfilled` callback parameter type.
        // This mirrors tsc's getAwaitedType which structurally inspects thenables.
        if let query::PromiseTypeKind::Object(_) =
            query::classify_promise_type(self.ctx.types, return_type)
            && let Some(awaited) = self.extract_awaited_type_from_thenable(return_type)
        {
            return Some(awaited);
        }

        // If we can't extract the type argument from a Promise-like type,
        // return None instead of ANY/UNKNOWN (consistent with Task 4-6 changes)
        // This allows the caller (await expressions) to use UNKNOWN as fallback
        None
    }

    /// Extract the awaited type from a thenable (object with a `then` method).
    ///
    /// When a `Promise<T>` type annotation is evaluated to an Object shape,
    /// the type argument T is embedded in the `then` method's callback parameter.
    /// This method extracts T by:
    /// 1. Finding the `then` property on the object
    /// 2. Getting its call signature
    /// 3. Extracting the first param of the `onfulfilled` callback (which is T)
    fn extract_awaited_type_from_thenable(&self, type_id: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::property_access::resolve_property_access;

        let then_type = resolve_property_access(self.ctx.types, type_id, "then").success_type()?;

        // Get call signatures of `then`
        let sigs = query::call_signatures_for_type(self.ctx.types, then_type)?;
        let first_sig = sigs.first()?;

        // The first parameter is `onfulfilled?: ((value: T) => ...) | null | undefined`.
        let onfulfilled_type = first_sig.params.first().map(|p| p.type_id)?;

        // Extract the first parameter of the onfulfilled callback.
        self.extract_first_param_from_callback(onfulfilled_type)
    }

    /// Extract the first parameter type from a callable/function type,
    /// handling unions of `(fn | null | undefined)`.
    fn extract_first_param_from_callback(&self, type_id: TypeId) -> Option<TypeId> {
        // Direct Callable
        if let Some(sigs) = query::call_signatures_for_type(self.ctx.types, type_id) {
            return sigs.first()?.params.first().map(|p| p.type_id);
        }
        // Direct Function
        if let Some(shape) = query::function_shape_for_type(self.ctx.types, type_id) {
            return shape.params.first().map(|p| p.type_id);
        }
        // Union: find first callable/function member
        if let Some(members) = query::union_members(self.ctx.types, type_id) {
            for member in &members {
                if let Some(sigs) = query::call_signatures_for_type(self.ctx.types, *member)
                    && let Some(first) = sigs.first()
                {
                    return first.params.first().map(|p| p.type_id);
                }
                if let Some(shape) = query::function_shape_for_type(self.ctx.types, *member) {
                    return shape.params.first().map(|p| p.type_id);
                }
            }
        }
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
        // Handle Lazy variant properly
        let sym_id = match query::classify_promise_type(self.ctx.types, base) {
            query::PromiseTypeKind::Lazy(def_id) => {
                // Use DefId -> SymbolId bridge
                self.ctx.def_to_symbol_id(def_id)?
            }
            query::PromiseTypeKind::TypeQuery(sym_ref) => SymbolId(sym_ref.0),
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
    /// the type argument from `MyPromise`<U>.
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
        let decl_idx = if symbol.value_declaration.is_some() {
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

        let type_alias = self.ctx.arena.get_type_alias_at(decl_idx)?;

        let mut bindings = Vec::new();
        if let Some(params) = &type_alias.type_parameters {
            if params.nodes.len() != args.len() {
                return None;
            }
            for (&param_idx, &arg) in params.nodes.iter().zip(args.iter()) {
                let param = self.ctx.arena.get_type_parameter_at(param_idx)?;
                let ident = self.ctx.arena.get_identifier_at(param.name)?;
                bindings.push((self.ctx.types.intern_string(&ident.escaped_text), arg));
            }
        } else if !args.is_empty() {
            return None;
        }

        // Check if the alias RHS is directly a Promise/PromiseLike type reference
        // before lowering (e.g., Promise<T> where Promise is from lib and might not fully resolve)
        if let Some(type_ref) = self.ctx.arena.get_type_ref_at(type_alias.type_node)
            && let Some(ident) = self.ctx.arena.get_identifier_at(type_ref.type_name)
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
        if let query::PromiseTypeKind::Application {
            base: lowered_base,
            args: lowered_args,
            ..
        } = query::classify_promise_type(self.ctx.types, lowered)
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
    /// the type argument from `MyPromise`<U>.
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
        let decl_idx = if symbol.value_declaration.is_some() {
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

        let class = self.ctx.arena.get_class_at(decl_idx)?;

        // Build type parameter bindings for this class
        let mut bindings = Vec::new();
        if let Some(params) = &class.type_parameters {
            if params.nodes.len() != args.len() {
                return None;
            }
            for (&param_idx, &arg) in params.nodes.iter().zip(args.iter()) {
                let param = self.ctx.arena.get_type_parameter_at(param_idx)?;
                let ident = self.ctx.arena.get_identifier_at(param.name)?;
                bindings.push((self.ctx.types.intern_string(&ident.escaped_text), arg));
            }
        } else if !args.is_empty() {
            return None;
        }

        // Check heritage clauses for extends Promise/PromiseLike
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let heritage = self.ctx.arena.get_heritage_clause_at(clause_idx)?;

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
        if let Some(members) = query::union_members(self.ctx.types, return_type) {
            for member in &members {
                if *member == TypeId::VOID || *member == TypeId::UNDEFINED {
                    return false;
                }
            }
        }

        true
    }

    /// Check if TS7030 (noImplicitReturns) should be skipped for this return type.
    ///
    /// TSC skips TS7030 for functions whose return type is or contains `void` or `any`.
    /// Top-level `undefined` also causes a skip, but `undefined` in a union does NOT.
    /// For unannotated functions, we only check top-level types because our inferred
    /// return types use `void` for implicit fall-through (TSC uses `undefined`).
    pub fn should_skip_no_implicit_return_check(
        &self,
        return_type: TypeId,
        has_type_annotation: bool,
    ) -> bool {
        if return_type == TypeId::VOID
            || return_type == TypeId::ANY
            || return_type == TypeId::UNDEFINED
        {
            return true;
        }

        // Only check unions for annotated return types. For unannotated functions,
        // our inferred return type includes `void` from implicit fall-through,
        // which would incorrectly trigger the skip.
        if has_type_annotation
            && let Some(members) = query::union_members(self.ctx.types, return_type)
        {
            for member in &members {
                if *member == TypeId::VOID || *member == TypeId::ANY {
                    return true;
                }
            }
        }

        false
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
            // Resolve Lazy references before trying to extract Promise<T>.
            // The return type annotation may be a Lazy(DefId) that hasn't been
            // evaluated to an Application yet.
            let resolved = self.resolve_ref_type(return_type);
            if let Some(inner) = self.promise_like_return_type_argument(resolved) {
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

    /// Check if a type is an Application (generic instantiation) whose base is definitively
    /// NOT the global Promise type.
    ///
    /// Used for TS1064: if the return type is `MyPromise<void>` (Application with base class
    /// "`MyPromise`"), we know it's not the global Promise and should emit TS1064.
    ///
    /// Returns false (uncertain) when:
    /// - The type is not an Application
    /// - The base is a type alias (aliases like `type P<T> = Promise<T>` resolve to Promise)
    /// - The base cannot be resolved to a symbol
    pub fn is_non_promise_application_type(&self, type_id: TypeId) -> bool {
        if let query::PromiseTypeKind::Application { base, .. } =
            query::classify_promise_type(self.ctx.types, type_id)
        {
            if self.is_global_promise_type(type_id) {
                return false;
            }
            // Check if the base is a type alias — aliases may resolve to Promise
            // (e.g., `type PromiseAlias<T> = Promise<T>`). In that case, we can't
            // definitively say it's not Promise, so return false to let the syntactic
            // check handle it.
            match query::classify_promise_type(self.ctx.types, base) {
                query::PromiseTypeKind::Lazy(def_id) => {
                    if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    {
                        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
                            return false; // Type alias — uncertain, use syntactic fallback
                        }
                        return true; // Class/interface — definitively not Promise
                    }
                    false // Can't resolve — uncertain
                }
                _ => true, // Non-Lazy base — definitively not global Promise
            }
        } else {
            false
        }
    }

    /// Check if a type alias ultimately resolves to the global Promise type.
    ///
    /// For `type MyPromise<T> = Promise<T>`, this returns true because the alias
    /// body is a `TypeReference` whose name is "Promise". Handles chains of aliases
    /// (e.g., `type A<T> = B<T>; type B<T> = Promise<T>`).
    fn type_alias_resolves_to_promise(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Find the type alias declaration among the symbol's declarations
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                continue;
            }
            let Some(type_alias) = self.ctx.arena.get_type_alias(decl_node) else {
                continue;
            };

            // Check if the alias body is a TypeReference
            let Some(body_node) = self.ctx.arena.get(type_alias.type_node) else {
                continue;
            };
            let Some(type_ref) = self.ctx.arena.get_type_ref(body_node) else {
                continue;
            };

            // Check if the type reference name is "Promise"
            let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
                continue;
            };
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                let name = ident.escaped_text.as_str();
                if Self::is_exactly_promise_name(name) {
                    return true;
                }
                // The alias body might reference another alias — resolve recursively
                if let Some(body_sym_id) = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&type_ref.type_name.0)
                    .copied()
                    && body_sym_id != sym_id
                {
                    // Avoid infinite loops
                    if let Some(body_symbol) = self.ctx.binder.get_symbol(body_sym_id)
                        && body_symbol.flags & symbol_flags::TYPE_ALIAS != 0
                    {
                        return self.type_alias_resolves_to_promise(body_sym_id, body_symbol);
                    }
                }
            }

            // Only check the first matching declaration
            break;
        }

        false
    }

    /// Strict syntactic check: is the return type annotation exactly `Promise<...>`?
    ///
    /// Unlike `return_type_annotation_looks_like_promise` (which matches any Promise-like name),
    /// this only matches exactly `Promise` — not `MyPromise`, not `X.MyPromise`.
    /// Used as a fallback for TS1064 emission when the resolved type loses its Application wrapper.
    pub fn return_type_annotation_is_exactly_promise(&self, type_annotation: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_annotation) else {
            return false;
        };

        if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
        {
            // Only match simple identifier "Promise" — not qualified names
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text.as_str() == "Promise";
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

    /// Extract the `TReturn` type argument from Generator<Y, R, N> or `AsyncGenerator`<Y, R, N>.
    ///
    /// For generator functions with explicit return types, the return statement
    /// should be checked against `TReturn` (the second type argument), not the full
    /// Generator/AsyncGenerator type.
    ///
    /// Returns `Some(TReturn)` if the type is a Generator/AsyncGenerator/Iterator/AsyncIterator
    /// type application with at least 2 type arguments, otherwise `None`.
    pub fn get_generator_return_type_argument(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.get_generator_arg_with_eval(type_id, 1)
    }

    /// Extract the `TYield` type argument from Generator<Y, R, N> or `AsyncGenerator`<Y, R, N>.
    ///
    /// For `yield expr` in a generator with an explicit return annotation,
    /// `expr` must be assignable to `TYield` (the first type argument).
    pub fn get_generator_yield_type_argument(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.get_generator_arg_with_eval(type_id, 0)
    }

    /// Extract the `TNext` type argument from Generator<Y, R, N> or `AsyncGenerator`<Y, R, N>.
    ///
    /// For yield expressions in a generator, the result type of `yield` is `TNext`
    /// (the type passed to `.next()`). This is the third type argument (index 2).
    pub fn get_generator_next_type_argument(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.get_generator_arg_with_eval(type_id, 2)
    }

    /// Shared helper: try direct extraction, then heritage, then shallow-expand
    /// type alias applications and retry.
    fn get_generator_arg_with_eval(&mut self, type_id: TypeId, arg_index: usize) -> Option<TypeId> {
        if let Some(result) = self.get_generator_arg_direct(type_id, arg_index) {
            return Some(result);
        }

        // Fallback: resolve through interface/class heritage clauses.
        if let Some(result) = self.resolve_generator_arg_from_heritage(type_id, arg_index, 0) {
            return Some(result);
        }

        // Fallback: shallow-expand type alias applications.
        // For `type MyGen<T> = Generator<..., T, ...> | AsyncGenerator<..., T, ...>`,
        // instantiate the alias body with args but don't recursively evaluate
        // Generator/AsyncGenerator into structural forms. This preserves the
        // Application wrappers we need to extract type args from.
        if let Some(expanded) = self.shallow_expand_type_alias(type_id) {
            return self.get_generator_arg_direct(expanded, arg_index);
        }

        None
    }

    /// Expand a type alias application by one level: substitute type args into the body
    /// without recursively evaluating the result. This preserves Application types like
    /// `Generator<Y,R,N>` in their wrapper form rather than expanding them to structural objects.
    fn shallow_expand_type_alias(&mut self, type_id: TypeId) -> Option<TypeId> {
        let (base, args) = query::application_info(self.ctx.types, type_id)?;
        if args.is_empty() {
            return None;
        }

        let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        if body_type == TypeId::ANY || body_type == TypeId::ERROR || type_params.is_empty() {
            return None;
        }

        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &type_params,
            &args,
        );
        let instantiated = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            body_type,
            &substitution,
        );
        if instantiated != type_id {
            Some(instantiated)
        } else {
            None
        }
    }

    /// Direct extraction of a type argument at `arg_index` from a generator-like Application type.
    /// Also handles union types (e.g., `Generator<Y,R,N> | AsyncGenerator<Y,R,N>`) by extracting
    /// the arg from each union member and combining them into a union.
    fn get_generator_arg_direct(&mut self, type_id: TypeId, arg_index: usize) -> Option<TypeId> {
        // Try direct extraction first (non-union case)
        if let Some(app) = query::type_application(self.ctx.types, type_id) {
            if !app.args.is_empty() && self.is_generator_like_base_type(app.base) {
                if arg_index < app.args.len() {
                    return Some(app.args[arg_index]);
                } else if arg_index == 1 && app.args.len() == 1 {
                    // IterableIterator<T>, AsyncIterableIterator<T> — only 1 type arg.
                    // TReturn defaults to `any` per the lib definitions.
                    return Some(TypeId::ANY);
                }
            }
            return None;
        }

        // Handle union types: extract the arg from each generator-like member
        let members = query::union_members(self.ctx.types, type_id)?;
        let mut extracted_args: Vec<TypeId> = Vec::new();
        for member in &members {
            if let Some(app) = query::type_application(self.ctx.types, *member)
                && !app.args.is_empty()
                && self.is_generator_like_base_type(app.base)
            {
                if arg_index < app.args.len() {
                    extracted_args.push(app.args[arg_index]);
                } else if arg_index == 1 && app.args.len() == 1 {
                    extracted_args.push(TypeId::ANY);
                }
            }
        }
        if extracted_args.is_empty() {
            return None;
        }
        // If all extracted args are the same, return it directly; otherwise union them
        if extracted_args.iter().all(|&a| a == extracted_args[0]) {
            Some(extracted_args[0])
        } else {
            Some(self.ctx.types.factory().union(extracted_args))
        }
    }

    /// Check if a type is a Generator-like base type (Generator, `AsyncGenerator`,
    /// Iterator, `AsyncIterator`, `IterableIterator`, `AsyncIterableIterator`,
    /// Iterable, `AsyncIterable`).
    fn is_generator_like_base_type(&mut self, type_id: TypeId) -> bool {
        // Fast path: Check for Lazy types to known Generator-like types
        {
            if let Some(def_id) = query::lazy_def_id(self.ctx.types, type_id) {
                // Use def_to_symbol_id to find the symbol
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                    && let Some(symbol) = self.get_symbol_globally(sym_id)
                    && Self::is_generator_like_name(&symbol.escaped_name)
                {
                    return true;
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
            "Iterable",
            "AsyncIterable",
        ] {
            // resolve_global_interface_type handles looking up in lib files and merging declarations
            if let Some(global_type) = self.resolve_global_interface_type(name)
                && global_type == type_id
            {
                return true;
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
                | "Iterable"
                | "AsyncIterable"
        )
    }

    /// Resolve through interface/class heritage clauses to extract a specific type argument
    /// from a generator-like base type.
    ///
    /// For `interface I1 extends Iterator<0, 1, 2> {}`, when given the TypeId of `I1`
    /// and `arg_index = 2`, this returns the TypeId for `2` (`TNext`).
    ///
    /// This enables extracting TYield/TReturn/TNext from indirect generator references
    /// used as generator function return type annotations.
    fn resolve_generator_arg_from_heritage(
        &mut self,
        type_id: TypeId,
        arg_index: usize,
        depth: u32,
    ) -> Option<TypeId> {
        // Guard against infinite recursion (e.g., circular heritage)
        if depth > 5 {
            return None;
        }

        // Get the DefId from a Lazy type
        let def_id = query::lazy_def_id(self.ctx.types, type_id)?;
        let sym_id = self.ctx.def_to_symbol_id(def_id)?;
        let symbol = self.get_symbol_globally(sym_id)?;
        let declarations = symbol.declarations.clone();

        for decl_idx in &declarations {
            let Some(decl_node) = self.ctx.arena.get(*decl_idx) else {
                continue;
            };

            // Check interface declarations
            if let Some(iface) = self.ctx.arena.get_interface(decl_node)
                && let Some(result) =
                    self.find_generator_arg_in_heritage(&iface.heritage_clauses, arg_index, depth)
            {
                return Some(result);
            }

            // Check class declarations
            if let Some(class) = self.ctx.arena.get_class(decl_node)
                && let Some(result) =
                    self.find_generator_arg_in_heritage(&class.heritage_clauses, arg_index, depth)
            {
                return Some(result);
            }
        }

        None
    }

    /// Walk heritage clauses to find a generator-like base and extract a type argument at `arg_index`.
    ///
    /// Heritage types are `ExpressionWithTypeArguments` nodes (e.g., `Iterator<0, 1, 2>`).
    /// We check syntactically if the heritage expression names a generator-like type,
    /// then extract the type argument at the requested index using `get_type_from_type_node`.
    fn find_generator_arg_in_heritage(
        &mut self,
        heritage_clauses: &Option<tsz_parser::parser::base::NodeList>,
        arg_index: usize,
        depth: u32,
    ) -> Option<TypeId> {
        let heritage_clauses = heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Heritage types are ExpressionWithTypeArguments nodes
                let (expr_idx, type_arguments) =
                    if let Some(expr_data) = self.ctx.arena.get_expr_type_args(type_node) {
                        (expr_data.expression, expr_data.type_arguments.clone())
                    } else {
                        (type_idx, None)
                    };

                // Check if the base expression names a generator-like type
                let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                    continue;
                };
                if let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                    && Self::is_generator_like_name(&ident.escaped_text)
                {
                    // Found generator-like heritage. Extract the type arg at arg_index.
                    if let Some(type_args) = &type_arguments {
                        if arg_index < type_args.nodes.len() {
                            return Some(self.get_type_from_type_node(type_args.nodes[arg_index]));
                        }
                        // arg_index == 1 with only 1 arg: TReturn defaults to `any`
                        if arg_index == 1 && type_args.nodes.len() == 1 {
                            return Some(TypeId::ANY);
                        }
                    }
                    return None; // Generator-like but missing the requested arg
                }

                // Non-generator heritage type — resolve its type and recurse through its heritage
                let heritage_base_type = self.get_type_of_node(expr_idx);
                if heritage_base_type != TypeId::ERROR
                    && let Some(result) = self.resolve_generator_arg_from_heritage(
                        heritage_base_type,
                        arg_index,
                        depth + 1,
                    )
                {
                    return Some(result);
                }
            }
        }

        None
    }

    /// Unwrap Promise<T> to T for async function return type checking.
    ///
    /// For async functions with declared return type `Promise<T>`, the function body
    /// should return values of type `T` (which get auto-wrapped in Promise).
    /// This function extracts T from Promise<T>.
    ///
    /// Returns None if the type is not a Promise type or if T cannot be extracted.
    pub fn unwrap_promise_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.promise_like_return_type_argument(type_id)
    }

    /// Unwrap Promise from an async function's return type for body checking.
    ///
    /// For contextually-typed async functions (no explicit annotation), the inferred
    /// return type may be `Promise<T>` or a union like `Promise<T> | StateMachine<T>`.
    /// This method unwraps each Promise member to produce the effective body return type:
    /// - `Promise<T>` → `T`
    /// - `Promise<T> | StateMachine<T>` → `T | StateMachine<T>`
    /// - Non-Promise types pass through unchanged.
    pub fn unwrap_async_return_type_for_body(&mut self, return_type: TypeId) -> TypeId {
        // Try simple unwrap first
        if let Some(unwrapped) = self.unwrap_promise_type(return_type) {
            return unwrapped;
        }
        // For unions, unwrap each Promise member individually
        if let Some(members) = query::union_members(self.ctx.types, return_type) {
            let mut new_members: Vec<TypeId> = Vec::new();
            for member in &members {
                if let Some(unwrapped) = self.unwrap_promise_type(*member) {
                    new_members.push(unwrapped);
                } else {
                    new_members.push(*member);
                }
            }
            return self.ctx.types.factory().union(new_members);
        }
        return_type
    }

    /// Check that `Generator<TYield, any, any>` (or `AsyncGenerator`) is assignable
    /// to the declared return type of an annotated generator function.
    ///
    /// This catches cases like `function* g(): WeirdIter {}` where `WeirdIter`
    /// extends `IterableIterator` with extra properties that `Generator<>` lacks.
    pub fn check_generator_return_type_assignability(
        &mut self,
        is_async: bool,
        yield_type: Option<TypeId>,
        declared_return_type: TypeId,
        error_node: NodeIndex,
    ) {
        if declared_return_type == TypeId::ANY
            || declared_return_type == TypeId::ERROR
            || declared_return_type == TypeId::VOID
            || self.type_contains_error(declared_return_type)
        {
            return;
        }
        // Direct standard iterator/generator return annotations are already handled
        // by body-level `return`/`yield` checking. The extra whole-signature
        // assignability check is only needed for custom iterator-like types
        // that add requirements beyond the standard library contracts.
        if let Some(type_ref) = self
            .ctx
            .arena
            .get(error_node)
            .and_then(|node| self.ctx.arena.get_type_ref(node))
            && let Some(name) = self.node_text(type_ref.type_name)
            && Self::is_generator_like_name(&name)
        {
            return;
        }
        // Also skip for types that extend a generator-like interface (e.g., `I1 extends Iterator<0, 1, 2>`)
        // BUT only when the interface has no own declared members in its body. If the interface
        // adds its own properties (e.g., `WeirdIter extends IterableIterator<number> { hello: string }`),
        // Generator<> may not satisfy it, so we must still perform the assignability check.
        if self
            .get_generator_return_type_argument(declared_return_type)
            .is_some()
        {
            // Check the AST declarations to see if the interface has own body members.
            // Use the same resolution path as resolve_generator_arg_from_heritage:
            // TypeId -> DefId -> SymbolId -> Symbol -> declarations -> interface body members.
            let def_id = query::lazy_def_id(self.ctx.types, declared_return_type);
            let sym_id = def_id.and_then(|d| self.ctx.def_to_symbol_id(d));
            let has_own_body_members = sym_id
                .and_then(|s| {
                    let symbol = self.get_symbol_globally(s)?;
                    let declarations = symbol.declarations.clone();
                    Some(declarations.iter().any(|decl_idx| {
                        self.ctx
                            .arena
                            .get(*decl_idx)
                            .and_then(|node| self.ctx.arena.get_interface(node))
                            .is_some_and(|iface| !iface.members.nodes.is_empty())
                    }))
                })
                .unwrap_or(false);
            if !has_own_body_members {
                return;
            }
        }
        let gen_name = if is_async {
            "AsyncGenerator"
        } else {
            "Generator"
        };
        // Ensure the lib type is loaded, then get a Lazy(DefId) reference
        // so the type displays as `Generator<...>` in error messages.
        let _resolved = self.resolve_lib_type_by_name(gen_name);
        let lazy_base = self.ctx.binder.file_locals.get(gen_name).map(|sym_id| {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx.types.factory().lazy(def_id)
        });
        if let Some(base) = lazy_base {
            let yield_t = yield_type.unwrap_or(TypeId::ANY);
            let inferred_gen = self
                .ctx
                .types
                .factory()
                .application(base, vec![yield_t, TypeId::ANY, TypeId::ANY]);
            self.ensure_relation_input_ready(inferred_gen);
            self.ensure_relation_input_ready(declared_return_type);
            self.check_assignable_or_report(inferred_gen, declared_return_type, error_node);
        }
    }
}
