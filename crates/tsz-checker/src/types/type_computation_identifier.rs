//! Identifier type computation for `CheckerState`.
//!
//! Resolves the type of identifier expressions by looking up symbols through
//! the binder, checking TDZ violations, validating definite assignment,
//! applying flow-based narrowing, and handling intrinsic/global names.

use crate::query_boundaries::type_computation_complex as query;
use crate::state::CheckerState;
use tracing::trace;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of an identifier expression.
    ///
    /// This function resolves the type of an identifier by:
    /// 1. Looking up the symbol through the binder
    /// 2. Getting the declared type of the symbol
    /// 3. Checking for TDZ (temporal dead zone) violations
    /// 4. Checking definite assignment for block-scoped variables
    /// 5. Applying flow-based type narrowing
    ///
    /// ## Symbol Resolution:
    /// - Uses `resolve_identifier_symbol` to find the symbol
    /// - Checks for type-only aliases (error if used as value)
    /// - Validates that symbol has a value declaration
    ///
    /// ## TDZ Checking:
    /// - Static block TDZ: variable used in static block before declaration
    /// - Computed property TDZ: variable in computed property before declaration
    /// - Heritage clause TDZ: variable in extends/implements before declaration
    ///
    /// ## Definite Assignment:
    /// - Checks if variable is definitely assigned before use
    /// - Only applies to block-scoped variables without initializers
    /// - Skipped for parameters, ambient contexts, and captured variables
    ///
    /// ## Flow Narrowing:
    /// - If definitely assigned, applies type narrowing based on control flow
    /// - Refines union types based on typeof guards, null checks, etc.
    ///
    /// ## Intrinsic Names:
    /// - `undefined` → UNDEFINED type
    /// - `NaN` / `Infinity` → NUMBER type
    /// - `Symbol` → Symbol constructor type (if available in lib)
    ///
    /// ## Global Value Names:
    /// - Returns ANY for available globals (Array, Object, etc.)
    /// - Emits error for unavailable ES2015+ types
    ///
    /// ## Error Handling:
    /// - Returns ERROR for:
    ///   - Type-only aliases used as values
    ///   - Variables used before declaration (TDZ)
    ///   - Variables not definitely assigned
    ///   - Static members accessed without `this`
    ///   - `await` in default parameters
    ///   - Unresolved names (with "cannot find name" error)
    /// - Returns ANY for unresolved imports (TS2307 already emitted)
    pub(crate) fn get_type_of_identifier(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };

        let name = &ident.escaped_text;

        // TS2496: 'arguments' cannot be referenced in an arrow function in ES5
        if name == "arguments" {
            // Track that this function body uses `arguments` (for JS implicit rest params)
            self.ctx.js_body_uses_arguments = true;

            // TS2815: 'arguments' cannot be referenced in property initializers
            // or class static initialization blocks. Must check BEFORE regular
            // function body check because arrow functions are transparent.
            if self.is_arguments_in_class_initializer_or_static_block(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                    diagnostic_codes::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                );
                return TypeId::ERROR;
            }

            use tsz_common::common::ScriptTarget;
            let is_es5_or_lower = matches!(
                self.ctx.compiler_options.target,
                ScriptTarget::ES3 | ScriptTarget::ES5
            );
            if is_es5_or_lower && self.is_arguments_in_arrow_function(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                );
                // Return ERROR to prevent fallthrough to normal resolution which would emit TS2304
                return TypeId::ERROR;
            }
            if is_es5_or_lower && self.is_arguments_in_async_non_arrow_function(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                );
                return TypeId::ERROR;
            }

            // Inside a regular (non-arrow) function body, `arguments` is the implicit
            // IArguments object, overriding any outer `arguments` declaration.
            // EXCEPT: if there's a LOCAL variable named "arguments" in the current function,
            // that shadows the built-in IArguments (e.g., `const arguments = this.arguments;`).
            if self.is_in_regular_function_body(idx) {
                // Check if there's a local "arguments" variable in the current function scope.
                // This handles shadowing: `const arguments = ...` takes precedence over IArguments.
                if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
                    // Found a symbol named "arguments". Check if it's declared locally
                    // in the current function (not in an outer scope).
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                        && !symbol.declarations.is_empty()
                    {
                        let decl_node = symbol.declarations[0];
                        // Find the enclosing function for both the reference and the declaration
                        if let Some(current_fn) = self.find_enclosing_function(idx) {
                            if let Some(decl_fn) = self.find_enclosing_function(decl_node) {
                                // If the declaration is in the same function scope, it shadows IArguments
                                if current_fn == decl_fn {
                                    trace!(
                                        name = name,
                                        idx = ?idx,
                                        sym_id = ?sym_id,
                                        "get_type_of_identifier: local 'arguments' variable shadows built-in IArguments"
                                    );
                                    // Fall through to normal resolution below - use the local variable
                                } else {
                                    // Declaration is in an outer scope - use built-in IArguments
                                    let lib_binders = self.get_lib_binders();
                                    if let Some(iargs_sym) = self
                                        .ctx
                                        .binder
                                        .get_global_type_with_libs("IArguments", &lib_binders)
                                    {
                                        return self.type_reference_symbol_type(iargs_sym);
                                    }
                                    return TypeId::ANY;
                                }
                            } else {
                                // Declaration not in a function (global) - use built-in IArguments
                                let lib_binders = self.get_lib_binders();
                                if let Some(iargs_sym) = self
                                    .ctx
                                    .binder
                                    .get_global_type_with_libs("IArguments", &lib_binders)
                                {
                                    return self.type_reference_symbol_type(iargs_sym);
                                }
                                return TypeId::ANY;
                            }
                        }
                    }
                } else {
                    // No symbol found at all - use built-in IArguments
                    let lib_binders = self.get_lib_binders();
                    if let Some(sym_id) = self
                        .ctx
                        .binder
                        .get_global_type_with_libs("IArguments", &lib_binders)
                    {
                        return self.type_reference_symbol_type(sym_id);
                    }
                    return TypeId::ANY;
                }
            }
        }

        // === CRITICAL FIX: Check type parameter scope FIRST ===
        // Type parameters in generic functions/classes/type aliases should be resolved
        // before checking any other scope. This is a common source of TS2304 false positives.
        // Examples:
        //   function foo<T>(x: T) { return x; }  // T should be found in the function body
        //   class C<U> { method(u: U) {} }  // U should be found in the class body
        //   type Pair<T> = [T, T];  // T should be found in the type alias definition
        if let Some(type_id) = self.lookup_type_parameter(name) {
            // Before emitting TS2693, check if the binder also has a value symbol
            // with the same name. In cases like `function f<A>(A: A)`, the parameter
            // `A` shadows the type parameter `A` in value position.
            let has_value_shadow = self
                .resolve_identifier_symbol(idx)
                .and_then(|sym_id| {
                    self.ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.flags & tsz_binder::symbol_flags::VALUE != 0)
                })
                .unwrap_or(false);
            if !has_value_shadow {
                // TS2693: Type parameters cannot be used as values
                // Example: function f<T>() { return T; }  // Error: T is a type, not a value
                self.error_type_parameter_used_as_value(name, idx);
                return type_id;
            }
            // Fall through to binder resolution — the value symbol takes precedence
        }

        // Resolve via binder persistent scopes for stateless lookup.
        if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
            // Reference tracking is handled by resolve_identifier_symbol wrapper
            trace!(
                name = name,
                idx = ?idx,
                sym_id = ?sym_id,
                "get_type_of_identifier: resolved symbol"
            );

            // TS7034: Check if this identifier references a pending implicit-any variable
            // from a nested function scope (i.e., the variable is captured by a closure).
            // If so, emit TS7034 at the declaration site.
            let mut emit_ts7005 = false;
            if self.ctx.pending_implicit_any_vars.contains_key(&sym_id) {
                let ref_fn = self.find_enclosing_function(idx);
                let decl_name_node = self.ctx.pending_implicit_any_vars[&sym_id];
                let decl_fn = self.find_enclosing_function(decl_name_node);
                if ref_fn != decl_fn {
                    // Variable is captured by a nested function — emit TS7034 at declaration.
                    let decl_name_node =
                        self.ctx.pending_implicit_any_vars.remove(&sym_id).unwrap();
                    self.ctx.reported_implicit_any_vars.insert(sym_id);
                    if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            decl_name_node,
                            diagnostic_codes::VARIABLE_IMPLICITLY_HAS_TYPE_IN_SOME_LOCATIONS_WHERE_ITS_TYPE_CANNOT_BE_DETERMIN,
                            &[&sym.escaped_name, "any"],
                        );
                    }
                    emit_ts7005 = true;
                }
            } else if self.ctx.reported_implicit_any_vars.contains(&sym_id) {
                let ref_fn = self.find_enclosing_function(idx);
                let decl_node = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .and_then(|sym| sym.declarations.first().copied());
                if let Some(decl_node) = decl_node {
                    let decl_fn = self.find_enclosing_function(decl_node);
                    if ref_fn != decl_fn {
                        emit_ts7005 = true;
                    }
                }
            }

            if emit_ts7005 && let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    idx,
                    diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE,
                    &[&sym.escaped_name, "any"],
                );
            }

            if self.is_type_only_import_equals_namespace_expr(idx) {
                self.error_namespace_used_as_value_at(name, idx);
                if let Some(sym_id) = self.resolve_identifier_symbol(idx)
                    && self.alias_resolves_to_type_only(sym_id)
                {
                    self.error_type_only_value_at(name, idx);
                }
                return TypeId::ERROR;
            }

            if self.alias_resolves_to_type_only(sym_id) {
                // Don't emit TS2693 in heritage clause context (e.g., `extends A`)
                if self.is_direct_heritage_type_reference(idx) {
                    return TypeId::ERROR;
                }
                // Don't emit TS2693 for export default/export = expressions
                if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                    && parent_ext.parent.is_some()
                    && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    {
                        return TypeId::ERROR;
                    }
                }
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            // Check symbol flags to detect type-only usage.
            // First try the main binder (fast path for local symbols).
            let local_symbol = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id));
            let flags = local_symbol.map_or(0, |s| s.flags);

            // TS2662: Bare identifier resolving to a static class member.
            // Static members must be accessed via `ClassName.member`, not as
            // bare identifiers.  The binder puts them in the class scope so
            // they resolve, but the checker must reject unqualified access.
            if (flags & tsz_binder::symbol_flags::STATIC) != 0
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && self.is_static_member(&class_info.member_nodes, name)
            {
                self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
                return TypeId::ERROR;
            }

            let has_type = (flags & tsz_binder::symbol_flags::TYPE) != 0;
            let has_value = (flags & tsz_binder::symbol_flags::VALUE) != 0;
            let is_type_alias = (flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0;
            trace!(
                name = name,
                flags = flags,
                has_type = has_type,
                has_value = has_value,
                is_interface = (flags & tsz_binder::symbol_flags::INTERFACE) != 0,
                "get_type_of_identifier: symbol flags"
            );
            let value_decl = local_symbol.map_or(NodeIndex::NONE, |s| s.value_declaration);
            let symbol_declarations = local_symbol
                .map(|s| s.declarations.clone())
                .unwrap_or_default();

            // Check for type-only symbols used as values
            // This includes:
            // 1. Symbols with TYPE flag but no VALUE flag (interfaces, type-only imports, etc.)
            // 2. Type aliases (never have VALUE, even if they reference a class)
            //
            // IMPORTANT: Only check is_interface if it has no VALUE flag.
            // Interfaces merged with namespaces DO have VALUE and should NOT error.
            //
            // CROSS-LIB MERGING: The same name may have TYPE in one lib file
            // (e.g., `interface Promise<T>` in es5.d.ts) and VALUE in another
            // (e.g., `declare var Promise` in es2015.promise.d.ts). When we find
            // a TYPE-only symbol, check if a VALUE exists elsewhere in libs.
            // Check for uninstantiated namespace used as a value (TS2708)
            let is_namespace = (flags & tsz_binder::symbol_flags::NAMESPACE_MODULE) != 0;
            let value_flags_except_module =
                tsz_binder::symbol_flags::VALUE & !tsz_binder::symbol_flags::VALUE_MODULE;
            let has_other_value = (flags & value_flags_except_module) != 0;
            if is_namespace && !has_other_value {
                let mut is_instantiated = false;
                tracing::debug!("checking is_instantiated for {name:?}");
                for decl_idx in &symbol_declarations {
                    if self.is_namespace_declaration_instantiated(*decl_idx) {
                        is_instantiated = true;
                        break;
                    }
                }
                if !is_instantiated {
                    if self.is_direct_heritage_type_reference(idx) {
                        return TypeId::ERROR;
                    }
                    if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                        && parent_ext.parent.is_some()
                        && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                    {
                        use tsz_parser::parser::syntax_kind_ext;
                        if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                            || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                            && access.expression == idx
                        {
                            // Defer diagnostics for `Ns.Member` to member-access handling so
                            // type-only member access can report TS2693 at the member site.
                            return self.get_type_of_symbol(sym_id);
                        }
                        if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                            || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                        {
                            return TypeId::ERROR;
                        }
                    }
                    self.error_namespace_used_as_value_at(name, idx);
                    return TypeId::ERROR;
                }
            }

            if is_type_alias || (has_type && !has_value) {
                trace!(
                    name = name,
                    sym_id = ?sym_id,
                    is_type_alias = is_type_alias,
                    has_type = has_type,
                    has_value = has_value,
                    "get_type_of_identifier: TYPE-only symbol, checking for VALUE in libs"
                );
                // Cross-lib merging: interface/type may be in one lib while VALUE
                // declaration is in another. Resolve by declaration node first to
                // avoid SymbolId collisions across binders.
                let value_type = self.type_of_value_symbol_by_name(name);
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type from type_of_value_symbol_by_name"
                );
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    trace!(
                        name = name,
                        value_type = ?value_type,
                        "get_type_of_identifier: using cross-lib VALUE type"
                    );
                    return self.check_flow_usage(idx, value_type, sym_id);
                }

                // Don't emit TS2693 in heritage clause context — but ONLY when the
                // identifier is the direct expression of an ExpressionWithTypeArguments
                // (e.g., `extends A`). If the identifier is nested deeper, such as
                // a function argument within the heritage expression (e.g.,
                // `extends factory(A)`), TS2693 should still fire.
                if self.is_direct_heritage_type_reference(idx) {
                    return TypeId::ERROR;
                }

                // Don't emit TS2693 for export default/export = expressions.
                // `export default InterfaceName` and `export = InterfaceName`
                // are valid TypeScript — they export the type binding.
                if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                    && parent_ext.parent.is_some()
                    && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    {
                        return TypeId::ERROR;
                    }
                }

                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }

            // NOTE: tsc 6.0 does NOT emit TS2585 based on target version alone.
            // ES2015+ globals (Symbol, Promise, Map, Set, etc.) may be available
            // even with target ES5 because lib.dom.d.ts transitively loads
            // lib.es2015.d.ts. We let the normal value-binding resolution below
            // determine if the value is truly available.

            // If the symbol wasn't found in the main binder (flags==0), it came
            // from a lib or cross-file binder.  For known ES2015+ global type
            // names (Symbol, Promise, Map, Set, etc.) we need to check whether
            // the lib binder's symbol is type-only.  Only do this for the known
            // set to avoid cross-binder ID collisions causing false TS2693 on
            // arbitrary user symbols from other files.
            if flags == 0 {
                use tsz_binder::lib_loader;
                if lib_loader::is_es2015_plus_type(name) {
                    let lib_binders = self.get_lib_binders();
                    let lib_flags = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .map_or(0, |s| s.flags);
                    let lib_has_type = (lib_flags & tsz_binder::symbol_flags::TYPE) != 0;
                    let lib_has_value = (lib_flags & tsz_binder::symbol_flags::VALUE) != 0;
                    if lib_has_type && !lib_has_value {
                        // Cross-lib merging: VALUE may be in a different lib binder.
                        // Resolve by declaration node first to avoid SymbolId collisions.
                        let value_type = self.type_of_value_symbol_by_name(name);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return self.check_flow_usage(idx, value_type, sym_id);
                        }
                        self.error_type_only_value_at(name, idx);
                        return TypeId::ERROR;
                    }
                }
            }

            // Merged interface+value symbols (e.g. `interface Promise<T>` +
            // `declare var Promise: PromiseConstructor`) must use the VALUE side
            // in value position. Falling back to interface type here causes
            // false TS2339/TS2351 on `Promise.resolve` / `new Promise(...)`.
            //
            // Merged interface+value symbols (e.g. Symbol interface + declare var Symbol: SymbolConstructor)
            // must use the VALUE side in value position. The *Constructor lookup below
            // handles finding the right type (SymbolConstructor, PromiseConstructor, etc.)
            let is_merged_interface_value =
                has_type && has_value && (flags & tsz_binder::symbol_flags::INTERFACE) != 0;
            // NOTE: tsc 6.0 does NOT emit TS2585 for ES2015+ globals based on
            // target alone. The value bindings from transitively loaded libs
            // (e.g. lib.dom.d.ts → lib.es2015.d.ts) are considered available.
            // The merged interface+value resolution below handles this correctly.
            if is_merged_interface_value {
                trace!(
                    name = name,
                    sym_id = ?sym_id,
                    value_decl = ?value_decl,
                    "get_type_of_identifier: merged interface+value path"
                );
                // NOTE: tsc 6.0 does NOT emit TS2585 based on target version.
                // Value declarations from transitively loaded libs are available.
                // Prefer value-declaration resolution for merged symbols so we pick
                // the constructor-side type (e.g. Promise -> PromiseConstructor).
                let mut value_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    for &decl_idx in &symbol_declarations {
                        let candidate = self.type_of_value_declaration_for_symbol(sym_id, decl_idx);
                        if candidate != TypeId::UNKNOWN && candidate != TypeId::ERROR {
                            value_type = candidate;
                            break;
                        }
                    }
                }
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    value_type = self.type_of_value_symbol_by_name(name);
                }
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    let direct_type = self.get_type_of_symbol(sym_id);
                    trace!(
                        name = name,
                        direct_type = ?direct_type,
                        "get_type_of_identifier: direct type from get_type_of_symbol"
                    );
                    if direct_type != TypeId::UNKNOWN && direct_type != TypeId::ERROR {
                        value_type = direct_type;
                    }
                }
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type after value-decl resolution"
                );
                // Lib globals often model value-side constructors through a sibling
                // `*Constructor` interface (Promise -> PromiseConstructor).
                // Prefer that when available to avoid falling back to the instance interface.
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type before *Constructor lookup"
                );
                let constructor_name = format!("{name}Constructor");
                trace!(
                    name = name,
                    constructor_name = %constructor_name,
                    "get_type_of_identifier: looking for *Constructor symbol"
                );
                // Use find_value_symbol_in_libs (not resolve_global_value_symbol) to get
                // the correct VALUE symbol. resolve_global_value_symbol can return the
                // wrong symbol when there are name collisions in file_locals.
                if let Some(constructor_sym_id) = self.find_value_symbol_in_libs(&constructor_name)
                {
                    trace!(
                        name = name,
                        constructor_sym_id = ?constructor_sym_id,
                        "get_type_of_identifier: found *Constructor symbol"
                    );
                    let constructor_type = self.get_type_of_symbol(constructor_sym_id);
                    trace!(
                        name = name,
                        constructor_type = ?constructor_type,
                        "get_type_of_identifier: *Constructor type"
                    );
                    if constructor_type != TypeId::UNKNOWN && constructor_type != TypeId::ERROR {
                        value_type = constructor_type;
                    }
                } else {
                    trace!(
                        name = name,
                        constructor_name = %constructor_name,
                        "get_type_of_identifier: find_value_symbol_in_libs returned None, trying resolve_lib_type_by_name"
                    );
                    if let Some(constructor_type) = self.resolve_lib_type_by_name(&constructor_name)
                        && constructor_type != TypeId::UNKNOWN
                        && constructor_type != TypeId::ERROR
                    {
                        trace!(
                            name = name,
                            constructor_type = ?constructor_type,
                            current_value_type = ?value_type,
                            "get_type_of_identifier: found *Constructor TYPE"
                        );
                        // Only use constructor_type if we don't already have a valid type.
                        // Don't let a fallback *Constructor type overwrite a correct direct type.
                        if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                            value_type = constructor_type;
                        }
                    } else {
                        trace!(
                            name = name,
                            constructor_name = %constructor_name,
                            "get_type_of_identifier: resolve_lib_type_by_name returned None/UNKNOWN/ERROR"
                        );
                    }
                }
                // For `declare var X: X` pattern (self-referential type annotation),
                // the type resolved through type_of_value_declaration may be incomplete
                // because the interface is resolved in a child checker with only one
                // lib arena. Use resolve_lib_type_by_name to get the complete interface
                // type merged from all lib files.
                if !self.ctx.lib_contexts.is_empty()
                    && self.is_self_referential_var_type(sym_id, value_decl, name)
                    && let Some(lib_type) = self.resolve_lib_type_by_name(name)
                    && lib_type != TypeId::UNKNOWN
                    && lib_type != TypeId::ERROR
                {
                    value_type = lib_type;
                }
                // Final fallback: if value_type is still a Lazy type (e.g., due to
                // check_variable_declaration overwriting the symbol_types cache with the
                // Lazy annotation type for `declare var X: X` patterns, and DefId
                // collisions corrupting the type_env), force recompute the symbol type.
                if query::lazy_def_id(self.ctx.types, value_type).is_some() {
                    self.ctx.symbol_types.remove(&sym_id);
                    let recomputed = self.get_type_of_symbol(sym_id);
                    if recomputed != value_type
                        && recomputed != TypeId::UNKNOWN
                        && recomputed != TypeId::ERROR
                    {
                        value_type = recomputed;
                    }
                }
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    return self.check_flow_usage(idx, value_type, sym_id);
                }
            }

            let declared_type = self.get_type_of_symbol(sym_id);
            // Check for TDZ violations (variable used before declaration in source order)
            if self.check_tdz_violation(sym_id, idx, name) {
                return TypeId::ERROR;
            }
            // Use check_flow_usage to integrate both DAA and type narrowing
            // This handles TS2454 errors and applies flow-based narrowing
            let flow_type = self.check_flow_usage(idx, declared_type, sym_id);
            trace!(
                ?flow_type,
                ?declared_type,
                "After check_flow_usage in get_type_of_identifier"
            );

            // FIX: Preserve readonly and other type modifiers from declared_type.
            // When declared_type has modifiers like ReadonlyType, we must preserve them
            // even if flow analysis infers a different type from the initializer.
            // IMPORTANT: Only apply this fix when there's NO contextual type to avoid interfering
            // with variance checking and assignability analysis.
            //
            // CRITICAL: Array element narrowing produces a genuinely different type that we must use.
            // Check if flow_type is a meaningful narrowing (not ANY/ERROR and different from declared_type).
            // If so, use it. Otherwise, preserve declared_type if it has special modifiers.
            let result_type = if self.ctx.contextual_type.is_none()
                && declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
            {
                // Check if we have genuine narrowing (different type that's not ANY/ERROR)
                let has_narrowing = flow_type != declared_type
                    && flow_type != TypeId::ANY
                    && flow_type != TypeId::ERROR;

                if has_narrowing {
                    // Check if this is "zombie freshness" - flow returning the widened
                    // version of our declared literal type. If widen(declared) == flow,
                    // use declared_type instead.
                    // IMPORTANT: Evaluate the declared type first to expand type aliases
                    // and lazy references, so widen_type can see the actual union members.
                    let evaluated_declared = self.evaluate_type_for_assignability(declared_type);
                    let widened_declared =
                        tsz_solver::widening::widen_type(self.ctx.types, evaluated_declared);
                    if widened_declared == flow_type {
                        declared_type
                    } else {
                        // Genuine narrowing (e.g., array element narrowing) - use narrowed type
                        flow_type
                    }
                } else {
                    // No narrowing or error - check if we should preserve declared_type
                    let has_index_sig = {
                        use tsz_solver::{IndexKind, IndexSignatureResolver};
                        let resolver = IndexSignatureResolver::new(self.ctx.types);
                        resolver.has_index_signature(declared_type, IndexKind::String)
                            || resolver.has_index_signature(declared_type, IndexKind::Number)
                    };
                    if query::is_readonly_type(self.ctx.types, declared_type) || has_index_sig {
                        declared_type
                    } else {
                        flow_type
                    }
                }
            } else {
                flow_type
            };

            // FIX: For mutable variables (let/var), always use declared_type instead of flow_type
            // to preserve literal type widening. Flow analysis may narrow back to literal types
            // from the initializer, but we need to keep the widened type (string, number, etc.)
            // const variables preserve their literal types through flow analysis.
            //
            // CRITICAL EXCEPTION: If flow_type is different from declared_type and not ERROR,
            // we should use flow_type. This allows discriminant narrowing to work for mutable
            // variables while preserving literal type widening in most cases.
            let is_const = self.is_const_variable_declaration(value_decl);
            let result_type = if !is_const {
                // Mutable variable (let/var)
                // If declared type has index signatures (either ObjectWithIndex or a resolved
                // type with index signatures like from a type alias), always preserve it.
                // This prevents false-positive TS2339 errors when accessing properties via
                // index signatures.
                let has_index_sig = {
                    use tsz_solver::{IndexKind, IndexSignatureResolver};
                    let resolver = IndexSignatureResolver::new(self.ctx.types);
                    resolver.has_index_signature(declared_type, IndexKind::String)
                        || resolver.has_index_signature(declared_type, IndexKind::Number)
                };
                if has_index_sig && (flow_type == declared_type || flow_type == TypeId::ERROR) {
                    declared_type
                } else if flow_type != declared_type && flow_type != TypeId::ERROR {
                    // Flow narrowed the type - but check if this is just the initializer
                    // literal being returned. For mutable variables without annotations,
                    // the declared type is already widened (e.g., STRING for "hi"),
                    // so if the flow type widens to the declared type, use declared_type.
                    let widened_flow = tsz_solver::widening::widen_type(self.ctx.types, flow_type);
                    if widened_flow == declared_type {
                        // Flow type is just the initializer literal - use widened declared type
                        declared_type
                    } else {
                        // Also check the reverse: if declared_type is a non-widened literal
                        // (e.g., "foo" from `declare var a: "foo"; let b = a`) and flow_type
                        // is its widened form (string), flow is just returning the widened
                        // version of our literal declared type - use declared_type.
                        // IMPORTANT: Evaluate the declared type first to expand type aliases
                        // and lazy references, so widen_type can see the actual union members.
                        let evaluated_declared =
                            self.evaluate_type_for_assignability(declared_type);
                        let widened_declared =
                            tsz_solver::widening::widen_type(self.ctx.types, evaluated_declared);
                        if widened_declared == flow_type {
                            declared_type
                        } else {
                            // Genuine narrowing (e.g., discriminant narrowing) - use narrowed type
                            flow_type
                        }
                    }
                } else {
                    // No narrowing or error - use declared type to preserve widening
                    declared_type
                }
            } else {
                // Const variable - use flow type (preserves literal type)
                result_type
            };

            // FIX: Flow analysis may return the original fresh type from the initializer expression.
            // For variable references, we must respect the widening that was applied during variable
            // declaration. If the symbol was widened (non-fresh), the flow result should also be widened.
            // This prevents "Zombie Freshness" where CFA bypasses the widened symbol type.
            if !self.ctx.compiler_options.sound_mode {
                use tsz_solver::freshness::{is_fresh_object_type, widen_freshness};
                if is_fresh_object_type(self.ctx.types, result_type) {
                    return widen_freshness(self.ctx.types, result_type);
                }
            }
            return result_type;
        }

        self.resolve_unresolved_identifier(idx, name)
    }

    /// Resolve an identifier that was NOT found in the binder's scope chain.
    ///
    /// Handles intrinsics (`undefined`, `NaN`, `Symbol`), known globals
    /// (`console`, `Math`, `Array`, etc.), static member suggestions, and
    /// "cannot find name" error reporting.
    fn resolve_unresolved_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        match name {
            "undefined" => TypeId::UNDEFINED,
            "NaN" | "Infinity" => TypeId::NUMBER,
            "Symbol" => self.resolve_symbol_constructor(idx, name),
            _ if self.is_known_global_value_name(name) => self.resolve_known_global(idx, name),
            _ => self.resolve_truly_unknown_identifier(idx, name),
        }
    }

    /// Resolve the `Symbol` constructor. Emits TS2583/TS2585 if Symbol is
    /// unavailable or type-only (ES5 target).
    fn resolve_symbol_constructor(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if !self.ctx.has_symbol_in_lib() {
            self.error_cannot_find_name_change_lib(name, idx);
            return TypeId::ERROR;
        }
        // NOTE: tsc 6.0 does NOT emit TS2585 based on target version alone.
        // Symbol may be available even with target ES5 via transitive lib loading.
        // Proceed to check if the value binding actually exists.
        let value_type = self.type_of_value_symbol_by_name(name);
        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
            return value_type;
        }
        self.error_type_only_value_at(name, idx);
        TypeId::ERROR
    }

    /// Resolve a known global value name (e.g. `console`, `Math`, `Array`).
    /// Tries binder `file_locals` and lib binders, then falls back to error reporting.
    fn resolve_known_global(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if self.is_nodejs_runtime_global(name) {
            // In CommonJS module mode, these globals are implicitly available
            if self.ctx.compiler_options.module.is_commonjs() {
                return TypeId::ANY;
            }
            // JS files implicitly have CommonJS globals (require, exports, module, etc.)
            // tsc never emits TS2580 for JS files — they're treated as CommonJS by default
            if self.is_js_file() {
                return TypeId::ANY;
            }
            // Otherwise, emit TS2580 suggesting @types/node installation
            self.error_cannot_find_name_install_node_types(name, idx);
            return TypeId::ERROR;
        }

        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return self.get_type_of_symbol(sym_id);
        }
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)
        {
            return self.get_type_of_symbol(sym_id);
        }

        self.emit_global_not_found_error(idx, name)
    }

    /// Emit an appropriate error when a known global is not found.
    fn emit_global_not_found_error(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        use crate::error_reporter::is_known_dom_global;
        use tsz_binder::lib_loader;

        if !self.ctx.has_lib_loaded() {
            if lib_loader::is_es2015_plus_type(name) {
                self.error_cannot_find_name_change_lib(name, idx);
            } else {
                self.error_cannot_find_name_at(name, idx);
            }
            return TypeId::ERROR;
        }

        if is_known_dom_global(name) {
            self.error_cannot_find_name_at(name, idx);
            return TypeId::ERROR;
        }
        if lib_loader::is_es2015_plus_type(name) {
            self.error_cannot_find_global_type(name, idx);
            return TypeId::ERROR;
        }

        let first_char = name.chars().next().unwrap_or('a');
        if first_char.is_uppercase() || self.is_known_global_value_name(name) {
            return TypeId::ANY;
        }

        // TS2693: Primitive type keywords used as values
        // TypeScript primitive type keywords (number, string, boolean, etc.) are language keywords
        // for types, not identifiers. When used in value position, emit TS2693.
        // NOTE: `symbol` is excluded — tsc never emits TS2693 for lowercase `symbol`.
        // Instead it emits TS2552 "Cannot find name 'symbol'. Did you mean 'Symbol'?"
        // Exception: in import equals module references (e.g., `import r = undefined`),
        // TS2503 is already emitted by check_namespace_import — don't also emit TS2693.
        if matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "void"
                | "undefined"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        ) {
            self.error_type_only_value_at(name, idx);
            return TypeId::ERROR;
        }

        if self.ctx.is_known_global_type(name) {
            self.error_cannot_find_global_type(name, idx);
        } else {
            self.error_cannot_find_name_at(name, idx);
        }
        TypeId::ERROR
    }

    /// Handle a truly unresolved identifier — not a type parameter, not in the
    /// binder, not a known global. Emits TS2304, TS2524, TS2662 as appropriate.
    fn resolve_truly_unknown_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        // Note: TS1212/1213/1214 strict-mode reserved word check is now handled
        // centrally in error_cannot_find_name_at to cover both value and type contexts.

        // Check static member suggestion (error 2662)
        if let Some(ref class_info) = self.ctx.enclosing_class.clone()
            && self.is_static_member(&class_info.member_nodes, name)
        {
            self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
            return TypeId::ERROR;
        }
        // TS2524: 'await' in default parameter
        if name == "await" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // TS2523: 'yield' in default parameter
        if name == "yield" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // Suppress TS2304 for unresolved imports (TS2307 was already emitted)
        if self.is_unresolved_import_symbol(idx) {
            return TypeId::ANY;
        }
        // Check known globals that might be missing
        if self.is_known_global_value_name(name) {
            return self.emit_global_not_found_error(idx, name);
        }
        // Always emit errors for primitive type keywords used as values,
        // regardless of report_unresolved_imports. These are built-in language
        // keywords, not cross-file identifiers that might be unresolved.
        if matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "symbol"
                | "void"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        ) {
            self.error_cannot_find_name_at(name, idx);
            return TypeId::ERROR;
        }
        // Suppress in single-file mode to prevent cascading false positives
        if !self.ctx.report_unresolved_imports {
            return TypeId::ANY;
        }
        self.error_cannot_find_name_at(name, idx);
        TypeId::ERROR
    }
}
