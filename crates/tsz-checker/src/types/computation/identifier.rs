//! Identifier type computation for `CheckerState`.
//!
//! Resolves the type of identifier expressions by looking up symbols through
//! the binder, checking TDZ violations, validating definite assignment,
//! applying flow-based narrowing, and handling intrinsic/global names.

use crate::context::should_resolve_jsdoc_for_file;
use crate::context::{PendingImplicitAnyKind, TypingRequest, is_js_file_name};
use crate::query_boundaries::common as common_query;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tracing::trace;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn has_recursive_alias_shape_for_flow_compare(&self, type_id: TypeId) -> bool {
        common_query::contains_lazy_or_recursive(self.ctx.types.as_type_database(), type_id)
    }

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
        self.get_type_of_identifier_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_identifier_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };

        let name = &ident.escaped_text;

        // TS1212: Check if identifier is a strict-mode reserved word used in expression context.
        // This fires for EVERY expression usage of reserved words like `interface`, `private`, etc.
        // Declaration-site TS1212 is handled separately in variable_checking/parameter_checker/etc.
        // We emit the error here but do NOT return early — the identifier may still resolve.
        if crate::state_checking::is_strict_mode_reserved_name(name)
            && self.is_strict_mode_for_node(idx)
            && self.ctx.checking_computed_property_name.is_none()
        {
            self.emit_strict_mode_reserved_word_error(idx, name, true);
        }

        if name == "arguments" {
            // Track that this function body uses `arguments` (for JS implicit rest params)
            self.ctx.js_body_uses_arguments = true;

            // TS2496: 'arguments' cannot be referenced in an arrow function in ES5.
            // Fires when `arguments` is inside an arrow that captures it from an outer
            // scope. Does NOT fire when `arguments` is a parameter of the immediate arrow
            // (e.g., `(arguments) => arguments`). tsc emits this and continues.
            if self.ctx.compiler_options.target.is_es5() && self.is_arguments_captured_by_arrow(idx)
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                );
            }

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

            // Check if there's a local variable named "arguments" that shadows the built-in.
            // If so, fall through to normal resolution.
            let has_local_shadow = if self.is_in_regular_function_body(idx) {
                if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                        && !symbol.declarations.is_empty()
                    {
                        let decl_node = symbol.declarations[0];
                        if let Some(current_fn) = self.find_enclosing_function(idx)
                            && let Some(decl_fn) = self.find_enclosing_function(decl_node)
                            && current_fn == decl_fn
                        {
                            trace!(
                                name = name,
                                idx = ?idx,
                                sym_id = ?sym_id,
                                "get_type_of_identifier: local 'arguments' variable shadows built-in IArguments"
                            );
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            // If not shadowed by a local variable, resolve to the built-in IArguments type.
            // This handles both regular functions and arrow functions (which are transparent
            // for `arguments` — they capture from the enclosing regular function).
            // At global scope or in type contexts (interfaces, type aliases), `arguments`
            // is not valid and should fall through to normal resolution (emitting TS2304).
            if !has_local_shadow && self.has_enclosing_regular_function(idx) {
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
                // The closest binder symbol has no VALUE flag (it's the type parameter
                // itself). But type parameters only shadow in type contexts — in value
                // contexts, an outer-scope value binding (e.g., a class) should be
                // accessible. Check if there's a VALUE symbol with the same name by
                // re-resolving while skipping TYPE_PARAMETER-only symbols.
                let lib_binders = self.get_lib_binders();
                let has_outer_value = self
                    .ctx
                    .binder
                    .resolve_identifier_with_filter(self.ctx.arena, idx, &lib_binders, |sym_id| {
                        self.ctx
                            .binder
                            .get_symbol_with_libs(sym_id, &lib_binders)
                            .is_some_and(|s| {
                                // Skip symbols that are ONLY type parameters
                                s.flags & tsz_binder::symbol_flags::VALUE != 0
                            })
                    })
                    .is_some();
                if has_outer_value {
                    // Fall through to binder resolution — the outer value takes
                    // precedence over the type parameter in expression context.
                } else {
                    // In heritage expression positions (`class C<T> extends T {}`),
                    // tsc reports TS2304 instead of TS2693 for type parameters.
                    if self.is_direct_heritage_type_reference(idx) {
                        if self.is_heritage_type_only_context(idx) {
                            return TypeId::ERROR;
                        }
                        // Route through boundary for TS2304/TS2552 with suggestion collection
                        self.report_not_found_at_boundary(
                            name,
                            idx,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        );
                        return TypeId::ERROR;
                    }
                    // TS2693: Type parameters cannot be used as values
                    // Example: function f<T>() { return T; }  // Error: T is a type, not a value
                    self.error_type_parameter_used_as_value(name, idx);
                    return type_id;
                }
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
            if let Some(pending) = self.ctx.pending_implicit_any_vars.get(&sym_id).copied() {
                if pending.kind == PendingImplicitAnyKind::CaptureOnly {
                    let ref_fn = self.find_enclosing_function(idx);
                    let decl_name_node = pending.name_node;
                    let decl_fn = self.find_enclosing_function(decl_name_node);
                    if ref_fn != decl_fn
                        && self.should_emit_pending_implicit_any_capture_diagnostic(idx, sym_id)
                    {
                        // Variable is captured by a nested function — emit TS7034 at declaration.
                        let decl_name_node = self
                            .ctx
                            .pending_implicit_any_vars
                            .remove(&sym_id)
                            .expect("sym_id was verified present via should_emit_pending_implicit_any_capture_diagnostic")
                            .name_node;
                        self.ctx
                            .reported_implicit_any_vars
                            .insert(sym_id, PendingImplicitAnyKind::CaptureOnly);
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
                }
            } else if self.ctx.reported_implicit_any_vars.get(&sym_id)
                == Some(&PendingImplicitAnyKind::CaptureOnly)
            {
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
                // When the import-equals resolves to a pure type (interface,
                // type alias) rather than a namespace/module, tsc emits TS2693
                // ("only refers to a type") instead of TS2708 ("cannot use
                // namespace as a value").
                if self.import_equals_export_is_pure_type(idx) {
                    self.report_wrong_meaning(
                        name,
                        idx,
                        sym_id,
                        crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    );
                } else {
                    self.report_wrong_meaning(
                        name,
                        idx,
                        sym_id,
                        crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    );
                    if let Some(sym_id) = self.resolve_identifier_symbol(idx)
                        && self.alias_resolves_to_type_only(sym_id)
                    {
                        self.report_wrong_meaning(
                            name,
                            idx,
                            sym_id,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        );
                    }
                }
                return TypeId::ERROR;
            }

            if self.alias_resolves_to_type_only(sym_id) {
                // Duplicate import-equals aliases may merge type-only and value targets
                // under one symbol. If a value import binding with the same local name
                // exists in the current source/module block, don't treat this as type-only.
                if self.source_file_has_value_import_binding_named(idx, name) {
                    return TypeId::ANY;
                }
                // Suppress TS1361/TS1362 only in type-only heritage contexts
                // (interface extends, class implements, declare class extends).
                // For regular class extends, TS1361 must fire because the extends
                // clause is a value context requiring a constructable runtime value.
                if self.is_heritage_type_only_context(idx) {
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
                // Don't emit TS1361 for computed property names in type/ambient
                // contexts (interfaces, type literals, abstract members, declare
                // class members). These don't emit runtime code.
                if self.is_in_ambient_computed_property_context() {
                    return TypeId::ERROR;
                }
                // Don't emit TS1361 for `typeof X` in type positions — the
                // identifier is used as a type query, not a runtime value.
                if self.is_in_type_query_context(idx) {
                    return TypeId::ERROR;
                }
                self.report_wrong_meaning(
                    name,
                    idx,
                    sym_id,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
                // Return the actual resolved type instead of ERROR so that
                // downstream checks (e.g., TS2349 for non-callable expressions)
                // can still fire. TSC emits TS1362 during name resolution but
                // continues to resolve the type normally.
                let resolved = self.get_type_of_symbol(sym_id);
                if resolved != TypeId::UNKNOWN && resolved != TypeId::ERROR {
                    return resolved;
                }
                return TypeId::ERROR;
            }
            // Check symbol flags to detect type-only usage.
            // First try the main binder (fast path for local symbols).
            let (flags, value_decl, symbol_declarations, is_umd_export) = {
                let local_symbol = self
                    .get_cross_file_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(sym_id));
                let flags = local_symbol.map_or(0, |s| s.flags);
                let value_decl = local_symbol.map_or(NodeIndex::NONE, |s| s.value_declaration);
                let symbol_declarations = local_symbol
                    .map(|s| s.declarations.clone())
                    .unwrap_or_default();
                let is_umd_export = local_symbol.is_some_and(|s| s.is_umd_export);
                (flags, value_decl, symbol_declarations, is_umd_export)
            };

            // TS2686: UMD global used as a value in a module file.
            // `export as namespace Foo` makes `Foo` globally visible, but in a module
            // file it must be imported — bare value references are an error.
            // Guards:
            // - Only emit for pure UMD aliases (ALIAS without VALUE). If the symbol
            //   also has a VALUE flag, a non-UMD global declaration exists for this name
            //   (e.g. `declare const React` in a global.d.ts), so it's a legitimate value.
            // - Skip identifiers that are part of the `export as namespace X` declaration
            //   itself — those are definition sites, not usage sites.
            // - Skip if any cross-file binder provides a non-UMD VALUE binding for the
            //   same name (e.g. `declare global { const React }` in another file).
            if is_umd_export
                && self.current_file_is_module_for_umd_global_access()
                && !self.ctx.compiler_options.allow_umd_global_access
                && (flags & symbol_flags::VALUE) == 0
                && !self.is_namespace_export_declaration_name(idx)
                && !self.is_export_assignment_expression_name(idx)
                && !self.has_non_umd_global_value(name)
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    idx,
                    diagnostic_codes::REFERS_TO_A_UMD_GLOBAL_BUT_THE_CURRENT_FILE_IS_A_MODULE_CONSIDER_ADDING_AN_IMPOR,
                    &[name],
                );
                // Don't return early — continue with type computation so downstream
                // checks don't cascade (tsc emits TS2686 but still resolves the type).
            }

            // TS2662: Bare identifier resolving to a static class member.
            // Static members must be accessed via `ClassName.member`, not as
            // bare identifiers.  The binder puts them in the class scope so
            // they resolve, but the checker must reject unqualified access.
            //
            // However, in tsc static members are NOT in the scope chain for
            // bare identifiers, so `static X = X` resolves the RHS `X` to the
            // outer scope.  We replicate this by re-resolving while skipping
            // the static member symbol; if an outer binding exists, use it.
            //
            // The STATIC flag on the symbol is sufficient proof — we don't need
            // to verify membership in the immediately enclosing class. This
            // handles nested class expressions inside static initializers where
            // the static member belongs to an outer class.
            if (flags & tsz_binder::symbol_flags::STATIC) != 0 {
                let lib_binders = self.get_lib_binders();
                let static_sym_id = sym_id;
                let outer_sym = self.ctx.binder.resolve_identifier_with_filter(
                    self.ctx.arena,
                    idx,
                    &lib_binders,
                    |candidate| candidate != static_sym_id,
                );
                if let Some(outer_sym_id) = outer_sym {
                    // Found an outer-scope binding — use it instead of
                    // emitting TS2662.
                    return self.get_type_of_symbol(outer_sym_id);
                }
                // Get the class name from the symbol's parent for the error message
                let class_name = if let Some(parent_sym) = self.ctx.binder.get_symbol(
                    self.ctx
                        .binder
                        .get_symbol(sym_id)
                        .map_or(tsz_binder::symbols::SymbolId::NONE, |s| s.parent),
                ) {
                    parent_sym.escaped_name.clone()
                } else if let Some(ref class_info) = self.ctx.enclosing_class {
                    class_info.name.clone()
                } else {
                    String::new()
                };
                self.error_cannot_find_name_static_member_at(name, &class_name, idx);
                return TypeId::ERROR;
            }

            // TS2475: 'const' enums can only be used in property or index access
            // expressions or the right hand side of an import/export assignment or
            // type query.
            if (flags & tsz_binder::symbol_flags::CONST_ENUM) != 0 {
                let is_valid_const_enum_usage = if let Some(parent_ext) =
                    self.ctx.arena.get_extended(idx)
                    && parent_ext.parent.is_some()
                    && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                        && self
                            .ctx
                            .arena
                            .get_access_expr(parent_node)
                            .is_some_and(|access| access.expression == idx)
                        || parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::TYPE_QUERY
                } else {
                    false
                };
                if !is_valid_const_enum_usage {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        idx,
                        diagnostic_messages::CONST_ENUMS_CAN_ONLY_BE_USED_IN_PROPERTY_OR_INDEX_ACCESS_EXPRESSIONS_OR_THE_RIGH,
                        diagnostic_codes::CONST_ENUMS_CAN_ONLY_BE_USED_IN_PROPERTY_OR_INDEX_ACCESS_EXPRESSIONS_OR_THE_RIGH,
                    );
                }
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
                    // For import aliases like `import * as A from "./a"`, the namespace
                    // object is always usable as a value (even if the module has no exports).
                    // Suppress TS2708 for these cases.
                    let has_alias = (flags & tsz_binder::symbol_flags::ALIAS) != 0;
                    if has_alias {
                        let lib_binders = self.get_lib_binders();
                        if let Some(symbol) =
                            self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                            && symbol.import_module.is_some()
                        {
                            return self.get_type_of_symbol(sym_id);
                        }
                    }
                    if let Some(value_type) = self.cross_file_global_value_type_by_name(name, true)
                    {
                        return value_type;
                    }
                    if self.has_non_umd_global_value(name) {
                        let value_type = self.type_of_value_symbol_by_name(name);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return value_type;
                        }
                        return self.get_type_of_symbol(sym_id);
                    }
                    if self.is_direct_heritage_type_reference(idx) {
                        return TypeId::ERROR;
                    }
                    // Suppress TS2708 when the identifier is part of an
                    // import-equals entity name (e.g., `import r = M.X`).
                    // Namespace references in import aliases are not value
                    // usages — they are just creating bindings.
                    if self.is_in_import_equals_entity_name(idx) {
                        return self.get_type_of_symbol(sym_id);
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
                            // If the local uninstantiated namespace shadows a global VALUE
                            // (e.g., `namespace Symbol {}` shadowing global `Symbol`),
                            // fall through to the global value so property access works.
                            let value_type = self.type_of_value_symbol_by_name(name);
                            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                                return value_type;
                            }
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
                    self.report_wrong_meaning(
                        name,
                        idx,
                        sym_id,
                        crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    );
                    return TypeId::ERROR;
                }
            }

            let has_alias = (flags & tsz_binder::symbol_flags::ALIAS) != 0;
            // When a symbol has both TYPE_ALIAS and VALUE flags (e.g.,
            // `type FAILURE = "FAILURE"; const FAILURE = "FAILURE";`),
            // the merged binder symbol has both flags. In value/expression
            // context, the VALUE side must take precedence — skip the
            // type-only branch so normal value resolution runs below.
            if (is_type_alias && !has_value) || (has_type && !has_value && !has_alias) {
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
                let lib_binders = self.get_lib_binders();
                let has_scoped_value_or_alias = self
                    .ctx
                    .binder
                    .resolve_identifier_with_filter(self.ctx.arena, idx, &lib_binders, |sid| {
                        self.ctx
                            .binder
                            .get_symbol_with_libs(sid, &lib_binders)
                            .is_some_and(|s| {
                                sid != sym_id
                                    && ((s.flags & tsz_binder::symbol_flags::VALUE) != 0
                                        || ((s.flags & tsz_binder::symbol_flags::ALIAS) != 0
                                            && !s.is_type_only))
                            })
                    })
                    .is_some();
                if has_scoped_value_or_alias {
                    return TypeId::ANY;
                }
                // If this file has a non-type-only import binding for the same local
                // name, prefer that value binding over a merged type-only symbol.
                if self.source_file_has_value_import_binding_named(idx, name) {
                    return TypeId::ANY;
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
                    && (parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION)
                {
                    return TypeId::ERROR;
                }

                // Don't emit TS2693 for type-only symbols referenced inside type
                // positions.  In multi-file mode, the checker may dispatch type-
                // position identifiers through get_type_of_identifier; emitting
                // TS2693 for type parameters or interfaces used inside type
                // annotations (TypeReference, TupleType, etc.) is always wrong.
                if self.is_identifier_in_type_position(idx) {
                    return TypeId::ERROR;
                }

                self.report_wrong_meaning(
                    name,
                    idx,
                    sym_id,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
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
                        self.report_wrong_meaning(
                            name,
                            idx,
                            sym_id,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        );
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
                let class_constructor_type = if (flags & tsz_binder::symbol_flags::CLASS) != 0 {
                    let direct_type = self.get_type_of_symbol(sym_id);
                    (direct_type != TypeId::UNKNOWN && direct_type != TypeId::ERROR)
                        .then_some(direct_type)
                } else {
                    None
                };
                let preferred_value_decl = self
                    .preferred_value_declaration(sym_id, value_decl, &symbol_declarations)
                    .unwrap_or(value_decl);
                // NOTE: tsc 6.0 does NOT emit TS2585 based on target version.
                // Value declarations from transitively loaded libs are available.
                // Prefer value-declaration resolution for merged symbols so we pick
                // the constructor-side type (e.g. Promise -> PromiseConstructor).
                let mut value_type =
                    self.type_of_value_declaration_for_symbol(sym_id, preferred_value_decl);
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    for &decl_idx in &symbol_declarations {
                        if decl_idx == preferred_value_decl {
                            continue;
                        }
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
                        value_type = if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR
                        {
                            constructor_type
                        } else if value_type == constructor_type {
                            value_type
                        } else if (flags & tsz_binder::symbol_flags::CLASS) != 0 {
                            self.merge_interface_types(value_type, constructor_type)
                        } else {
                            constructor_type
                        };
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
                        } else if value_type != constructor_type
                            && (flags & tsz_binder::symbol_flags::CLASS) != 0
                        {
                            value_type = self.merge_interface_types(value_type, constructor_type);
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
                    && self.is_self_referential_var_type(sym_id, preferred_value_decl, name)
                    && let Some(lib_type) = self.resolve_lib_type_by_name(name)
                    && lib_type != TypeId::UNKNOWN
                    && lib_type != TypeId::ERROR
                {
                    value_type = lib_type;
                }
                if let Some(class_constructor_type) = class_constructor_type {
                    value_type = if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                        class_constructor_type
                    } else if value_type == class_constructor_type {
                        value_type
                    } else {
                        self.merge_interface_types(class_constructor_type, value_type)
                    };
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

            // Merged namespace+value symbols (e.g. `declare namespace Foo { ... }`
            // plus `declare const Foo: ...`) should use the concrete value declaration
            // in value position. Falling back to `get_type_of_symbol` here can return
            // the namespace side instead of the annotated callable/component value type.
            let is_merged_namespace_value = has_value
                && value_decl.is_some()
                && (flags
                    & (symbol_flags::MODULE
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE))
                    != 0
                && (flags
                    & (symbol_flags::INTERFACE
                        | symbol_flags::CLASS
                        | symbol_flags::ENUM
                        | symbol_flags::TYPE_ALIAS))
                    == 0;
            if is_merged_namespace_value
                && let Some(preferred_value_decl) =
                    self.preferred_value_declaration(sym_id, value_decl, &symbol_declarations)
            {
                let value_type =
                    self.type_of_value_declaration_for_symbol(sym_id, preferred_value_decl);
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    return self.check_flow_usage(idx, value_type, sym_id);
                }
            }

            // Merged TYPE_ALIAS + VALUE symbols: when a user-defined value (e.g.,
            // `declare const Readonly: unique symbol`) shares a name with a global
            // type alias (e.g., `type Readonly<T> = ...` from lib.d.ts), the binder
            // merges them into one symbol. `get_type_of_symbol` may return the lib's
            // type alias rather than the user's value type. In value/expression
            // context, resolve the VALUE declaration directly.
            let declared_type = if is_type_alias
                && has_value
                && (flags & symbol_flags::INTERFACE) == 0
                && (flags & symbol_flags::CLASS) == 0
            {
                // Try to find and resolve the value declaration from the symbol's
                // declarations in the current (user) arena.
                let mut value_type_found = TypeId::UNKNOWN;
                for &decl_idx in &symbol_declarations {
                    if decl_idx.is_none() {
                        continue;
                    }
                    if let Some(node) = self.ctx.arena.get(decl_idx) {
                        use tsz_parser::parser::syntax_kind_ext;
                        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                            || node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                            || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        {
                            // Use type_of_value_declaration directly since we verified
                            // the node is in the current arena. Going through
                            // type_of_value_declaration_for_symbol would look up
                            // symbol_arenas, which may point to the lib arena for
                            // merged symbols, causing a cross-arena collision.
                            let vt = self.type_of_value_declaration(decl_idx);
                            if vt != TypeId::UNKNOWN && vt != TypeId::ERROR {
                                value_type_found = vt;
                                break;
                            }
                        }
                    }
                }
                if value_type_found != TypeId::UNKNOWN {
                    value_type_found
                } else {
                    self.get_type_of_symbol(sym_id)
                }
            } else if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & symbol_flags::ENUM) != 0
                && (symbol.flags & symbol_flags::ENUM_MEMBER) == 0
            {
                self.enum_object_type(sym_id)
                    .inspect(|&enum_obj| {
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        self.ctx
                            .definition_store
                            .register_type_to_def(enum_obj, def_id);
                    })
                    .unwrap_or_else(|| self.get_type_of_symbol(sym_id))
            } else if (flags & symbol_flags::CLASS) != 0
                && (flags & symbol_flags::FUNCTION) == 0
                && has_value
                && value_decl.is_some()
                && self.ctx.symbol_resolution_set.contains(&sym_id)
            {
                // CLASS symbols in value position during circular resolution: use
                // type_of_value_declaration_for_symbol to get the constructor type.
                // The normal get_type_of_symbol path returns Lazy(def_id) during
                // circularity, which resolves to the instance type rather than the
                // constructor type (typeof C). This causes false TS2339 errors for
                // valid static member access in instance property initializers
                // (e.g., `x = C.bar` where bar is a static member).
                // type_of_value_declaration_for_symbol calls get_class_constructor_type
                // which has its own independent resolution mechanism that avoids the
                // cycle and returns the correct constructor type.
                // Note: When a class merges with a function declaration, we must use
                // get_type_of_symbol to get the merged type with both call and construct
                // signatures.
                let preferred_value_decl = self
                    .preferred_value_declaration(sym_id, value_decl, &symbol_declarations)
                    .unwrap_or(value_decl);
                let ctor_type =
                    self.type_of_value_declaration_for_symbol(sym_id, preferred_value_decl);
                if ctor_type != TypeId::UNKNOWN && ctor_type != TypeId::ERROR {
                    ctor_type
                } else {
                    self.get_type_of_symbol(sym_id)
                }
            } else {
                self.get_type_of_symbol(sym_id)
            };
            let preferred_cross_file_type = if self.ctx.is_js_file()
                && self.ctx.should_resolve_jsdoc()
                && (flags
                    & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                        | symbol_flags::BLOCK_SCOPED_VARIABLE))
                    != 0
            {
                self.preferred_non_js_cross_file_global_value_type(name, sym_id)
            } else {
                None
            };
            let declared_type = preferred_cross_file_type.unwrap_or(declared_type);
            // Check for TDZ violations (variable used before declaration in source order)
            if self.check_tdz_violation(sym_id, idx, name, true) {
                return TypeId::ERROR;
            }
            // Use check_flow_usage to integrate both DAA and type narrowing
            // This handles TS2454 errors and applies flow-based narrowing
            let flow_type = self.check_flow_usage(idx, declared_type, sym_id);
            self.maybe_emit_pending_evolving_array_diagnostic(idx, sym_id, flow_type);
            trace!(
                ?flow_type,
                ?declared_type,
                "After check_flow_usage in get_type_of_identifier"
            );

            if let Some(preferred_cross_file_type) = preferred_cross_file_type {
                return self.instantiate_callable_result_from_request(
                    idx,
                    preferred_cross_file_type,
                    request,
                );
            }
            // FIX: Preserve readonly and other type modifiers from declared_type.
            // When declared_type has modifiers like ReadonlyType, we must preserve them
            // even if flow analysis infers a different type from the initializer.
            // IMPORTANT: Only apply this fix when there's NO contextual type to avoid interfering
            // with variance checking and assignability analysis.
            //
            // CRITICAL: Array element narrowing produces a genuinely different type that we must use.
            // Check if flow_type is a meaningful narrowing (not ANY/ERROR and different from declared_type).
            // If so, use it. Otherwise, preserve declared_type if it has special modifiers.
            let result_type = if request.contextual_type.is_none()
                && declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
            {
                // Check if we have genuine narrowing (different type that's not ANY/ERROR)
                let has_narrowing = flow_type != declared_type
                    && flow_type != TypeId::ANY
                    && flow_type != TypeId::ERROR;

                if has_narrowing {
                    if self.has_recursive_alias_shape_for_flow_compare(declared_type) {
                        flow_type
                    } else {
                        // Check if this is "zombie freshness" - flow returning the widened
                        // version of our declared literal type. If widen(declared) == flow,
                        // use declared_type instead.
                        // IMPORTANT: Evaluate the declared type first to expand type aliases
                        // and lazy references, so widen_type can see the actual union members.
                        let evaluated_declared =
                            self.evaluate_type_for_assignability(declared_type);
                        let widened_declared =
                            tsz_solver::widening::widen_type(self.ctx.types, evaluated_declared);
                        if widened_declared == flow_type {
                            declared_type
                        } else {
                            // Genuine narrowing (e.g., array element narrowing) - use narrowed type
                            flow_type
                        }
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
            let mut binding_element_decl = NodeIndex::NONE;
            let mut enclosing_decl = value_decl;
            for _ in 0..32 {
                let Some(current_node) = self.ctx.arena.get(enclosing_decl) else {
                    break;
                };
                if current_node.kind == syntax_kind_ext::BINDING_ELEMENT
                    && binding_element_decl.is_none()
                {
                    binding_element_decl = enclosing_decl;
                }
                if current_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                    || current_node.kind == syntax_kind_ext::PARAMETER
                {
                    break;
                }
                let Some(ext) = self.ctx.arena.get_extended(enclosing_decl) else {
                    break;
                };
                enclosing_decl = ext.parent;
                if enclosing_decl.is_none() {
                    break;
                }
            }
            let is_const = enclosing_decl.is_some()
                && self.ctx.arena.is_const_variable_declaration(enclosing_decl);
            let is_parameter_binding = self
                .ctx
                .arena
                .get(enclosing_decl)
                .is_some_and(|decl_node| decl_node.kind == syntax_kind_ext::PARAMETER);
            let has_enclosing_binding_default = binding_element_decl.is_some() && {
                let mut current = binding_element_decl;
                let mut found = false;
                for _ in 0..32 {
                    let Some(current_node) = self.ctx.arena.get(current) else {
                        break;
                    };
                    if current_node.kind == syntax_kind_ext::BINDING_ELEMENT
                        && let Some(binding) = self.ctx.arena.get_binding_element(current_node)
                        && binding.initializer.is_some()
                    {
                        found = true;
                        break;
                    }
                    let Some(ext) = self.ctx.arena.get_extended(current) else {
                        break;
                    };
                    current = ext.parent;
                    if current.is_none() {
                        break;
                    }
                }
                found
            };
            let result_type = if !is_const && !is_parameter_binding {
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
                    } else if self.has_recursive_alias_shape_for_flow_compare(declared_type) {
                        flow_type
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
                // Const variable - usually use flow type to preserve literal type.
                // In JS/checkJs, `Object.defineProperty(x, ...)` augments the declared
                // object shape after the initializer is analyzed, so flow can still see
                // the original `{}` initializer while the symbol type has the richer
                // property surface. Prefer the declared type in that case.
                if (self.ctx.is_js_file()
                    && declared_type != TypeId::ANY
                    && declared_type != TypeId::ERROR
                    && flow_type != declared_type
                    && tsz_solver::type_queries::get_object_shape(self.ctx.types, declared_type)
                        .is_some())
                    || (request.contextual_type.is_some()
                        && has_enclosing_binding_default
                        && flow_type != TypeId::ERROR
                        && self.is_assignable_to(flow_type, declared_type))
                {
                    declared_type
                } else {
                    result_type
                }
            };
            // FIX: Flow analysis may return the original fresh type from the initializer expression.
            // For variable references, we must respect the widening that was applied during variable
            // declaration. If the symbol was widened (non-fresh), the flow result should also be widened.
            // This prevents "Zombie Freshness" where CFA bypasses the widened symbol type.
            if !self.ctx.compiler_options.sound_mode {
                use crate::query_boundaries::common::{is_fresh_object_type, widen_freshness};
                if is_fresh_object_type(self.ctx.types, result_type) {
                    let widened = widen_freshness(self.ctx.types, result_type);
                    return self.instantiate_callable_result_from_request(idx, widened, request);
                }
            }
            return self.instantiate_callable_result_from_request(idx, result_type, request);
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
        // Route through wrong-meaning boundary: Symbol is a type-only name
        use crate::query_boundaries::name_resolution::NameLookupKind;
        self.report_wrong_meaning_diagnostic(name, idx, NameLookupKind::Type);
        TypeId::ERROR
    }

    /// Resolve a known global value name (e.g. `console`, `Math`, `Array`).
    /// Tries binder `file_locals` and lib binders, then falls back to error reporting.
    fn resolve_known_global(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if self.is_nodejs_runtime_global(name) {
            if self.is_private_name_access_base(idx) {
                self.error_at_node_msg(
                    idx,
                    crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                    &[name],
                );
                return TypeId::ERROR;
            }
            // In CommonJS module mode, `exports` is implicitly available as the module namespace.
            // Other node globals (module, require, __dirname, __filename) still need @types/node;
            // tsc emits TS2591 for them even in CommonJS mode when type definitions are absent.
            if self.ctx.compiler_options.module.is_commonjs() && name == "exports" {
                return self.current_file_commonjs_namespace_type();
            }
            // JS files implicitly have CommonJS globals (require, exports, module, etc.)
            // tsc never emits TS2580 for JS files — they're treated as CommonJS by default
            if self.is_js_file() {
                if name == "exports" {
                    return self.current_file_commonjs_namespace_type();
                }
                return TypeId::ANY;
            }
            // Otherwise, emit TS2591 suggesting @types/node installation
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

    /// Check whether an identifier is the base of a private-name access (`obj.#field`).
    pub(crate) fn is_private_name_access_base(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let mut current = idx;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                && let Some(member) = self.ctx.arena.get(access.name_or_argument)
                && member.kind == SyntaxKind::PrivateIdentifier as u16
            {
                let mut chain_expr = access.expression;
                let mut chain_guard = 0;
                while chain_expr.is_some() {
                    chain_guard += 1;
                    if chain_guard > 256 {
                        break;
                    }
                    if chain_expr == idx {
                        return true;
                    }
                    let Some(chain_node) = self.ctx.arena.get(chain_expr) else {
                        break;
                    };
                    if chain_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        && chain_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    {
                        break;
                    }
                    let Some(chain_access) = self.ctx.arena.get_access_expr(chain_node) else {
                        break;
                    };
                    chain_expr = chain_access.expression;
                }
            }
            current = parent_idx;
        }
        false
    }

    /// Emit an appropriate error when a known global is not found.
    ///
    /// Routes through the environment capability boundary (`diagnose_missing_name`)
    /// to determine the appropriate diagnostic.
    fn emit_global_not_found_error(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        use crate::query_boundaries::environment::CapabilityDiagnostic;

        if !self.ctx.capabilities.has_lib {
            if let Some(CapabilityDiagnostic::MissingEs2015Type { .. }) =
                self.ctx.capabilities.diagnose_missing_name(name)
            {
                self.error_cannot_find_name_change_lib(name, idx);
            } else {
                // Route through boundary for TS2304/TS2552 with suggestion collection
                self.report_not_found_at_boundary(
                    name,
                    idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
            }
            return TypeId::ERROR;
        }

        match self.ctx.capabilities.diagnose_missing_name(name) {
            Some(CapabilityDiagnostic::MissingDomGlobal { .. }) => {
                // Route through boundary for TS2304/TS2552 with suggestion collection
                self.report_not_found_at_boundary(
                    name,
                    idx,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
                return TypeId::ERROR;
            }
            Some(CapabilityDiagnostic::MissingEs2015Type { .. }) => {
                self.error_cannot_find_global_type(name, idx);
                return TypeId::ERROR;
            }
            _ => {}
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
            // Suppress TS2693 when this identifier is the expression of an element
            // access with a missing argument (e.g., `new number[]`).  The parser
            // already emits TS1011 and tsc does not emit TS2693 in this case.
            if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                && parent_ext.parent.is_some()
                && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
            {
                use tsz_parser::parser::syntax_kind_ext;
                if parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                    && access.name_or_argument.is_none()
                {
                    return TypeId::ERROR;
                }
            }
            // Route through wrong-meaning boundary: primitive keyword is type-only
            use crate::query_boundaries::name_resolution::NameLookupKind;
            self.report_wrong_meaning_diagnostic(name, idx, NameLookupKind::Type);
            return TypeId::ERROR;
        }

        if self.ctx.is_known_global_type(name) {
            self.error_cannot_find_global_type(name, idx);
        } else {
            // Route through boundary for TS2304/TS2552 with suggestion collection
            self.report_not_found_at_boundary(
                name,
                idx,
                crate::query_boundaries::name_resolution::NameLookupKind::Value,
            );
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
        // TS2663: Check instance member suggestion — "Did you mean 'this.X'?"
        // When an unresolved name matches an instance member of the enclosing class
        // AND we're in a non-static context where `this` refers to the class instance,
        // suggest 'this.X'. Don't suggest when:
        // - We're in a static method (this refers to constructor)
        // - We're inside a regular function expression (this is rebound)
        if let Some(ref class_info) = self.ctx.enclosing_class.clone()
            && !class_info.in_static_member
            && self.is_instance_member(&class_info.member_nodes, name)
            && !self.has_regular_function_boundary_to_class(idx)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                &[name],
            );
            self.error_at_node(
                idx,
                &message,
                diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
            );
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
        if self.is_root_identifier_of_js_prototype_assignment(idx) {
            return TypeId::ANY;
        }
        // Check known globals that might be missing
        if self.is_known_global_value_name(name) {
            return self.emit_global_not_found_error(idx, name);
        }
        // Primitive type keywords used as values should emit TS2693 ("only
        // refers to a type, but is being used as a value here"), not TS2304.
        // These are built-in language keywords — they exist as types but
        // cannot be used as runtime values.
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
            // Route through wrong-meaning boundary: primitive keyword is type-only
            use crate::query_boundaries::name_resolution::NameLookupKind;
            self.report_wrong_meaning_diagnostic(name, idx, NameLookupKind::Type);
            return TypeId::ERROR;
        }
        // Suppress in single-file mode to prevent cascading false positives
        if !self.ctx.report_unresolved_imports {
            return TypeId::ANY;
        }
        // Route through the unified name resolution boundary for TS2304/TS2552
        let req = crate::query_boundaries::name_resolution::NameResolutionRequest::value(name, idx);
        let result = self.resolve_name_structured(&req);
        match result {
            Ok(_) => {
                // Symbol found — shouldn't happen since binder already failed,
                // but avoid false diagnostic
                TypeId::ERROR
            }
            Err(failure) => {
                self.report_name_resolution_failure(&req, &failure);
                TypeId::ERROR
            }
        }
    }

    /// Check if there's a regular function expression (non-arrow) between `idx`
    /// and the enclosing class declaration. Regular functions rebind `this`, so
    /// `this.member` wouldn't refer to the class instance. Arrow functions and
    /// method declarations preserve the outer `this`.
    fn has_regular_function_boundary_to_class(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = idx;
        let mut guard = 0;
        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                // Hit the class boundary — no regular function in between
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    return false;
                }
                // Regular function expression rebinds `this`
                if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                {
                    return true;
                }
                // Arrow functions and method declarations are fine (preserve `this`)
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        false
    }

    /// Returns `true` if any cross-file binder provides a non-UMD VALUE binding for
    /// `name`. This handles cases like `declare global { const React }` where a separate
    /// declaration provides a legitimate global value binding alongside the UMD export.
    pub(crate) fn has_non_umd_global_value(&self, name: &str) -> bool {
        // VALUE_MODULE alone means "I'm a namespace declaration" — not a real
        // runtime value.  Exclude it so uninstantiated namespaces (which only
        // carry VALUE_MODULE) do not suppress TS2708 / TS2686 diagnostics.
        let real_value_flags = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;

        // A symbol counts as a non-UMD global value if:
        // 1. It has real value flags AND is not a UMD export, OR
        // 2. It is a UMD export that has been merged with a non-UMD value declaration
        //    (e.g., `export as namespace X` merged with `declare global { const X }`).
        //    In this case, the symbol retains `is_umd_export = true` but gains
        //    VARIABLE flags from the global augmentation.
        let is_non_umd_value = |sym: &tsz_binder::Symbol| -> bool {
            let has_real_value = (sym.flags & real_value_flags) != 0;
            if !has_real_value {
                return false;
            }
            if !sym.is_umd_export {
                return true;
            }
            // UMD export merged with a variable declaration from `declare global`
            (sym.flags & symbol_flags::VARIABLE) != 0
        };

        // Check lib_contexts (lib files + some user files)
        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(sym) = lib_ctx.binder.get_symbol(sym_id)
                && is_non_umd_value(sym)
            {
                return true;
            }
        }
        // Check all_binders (all project files in multi-file mode)
        // Use global_file_locals_index for O(1) lookup
        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
        {
            let all_binders = self.ctx.all_binders.as_ref();
            for &(file_idx, sym_id) in entries {
                let binder = all_binders
                    .and_then(|binders| binders.get(file_idx))
                    .map(|binder| binder.as_ref())
                    .unwrap_or(self.ctx.binder);
                if let Some(sym) = binder.get_symbol(sym_id)
                    && is_non_umd_value(sym)
                {
                    return true;
                }
            }
        } else if let Some(ref all_binders) = self.ctx.all_binders {
            for binder in all_binders.iter() {
                if let Some(sym_id) = binder.file_locals.get(name)
                    && let Some(sym) = binder.get_symbol(sym_id)
                    && is_non_umd_value(sym)
                {
                    return true;
                }
            }
        }

        // Check global augmentations (`declare global { namespace X { ... } }`).
        // A `declare global` namespace with value members provides a legitimate
        // global value binding that should suppress TS2686 / TS2708, even though
        // the namespace symbol only carries VALUE_MODULE (excluded above).
        // tsc merges these augmentations into the global symbol, giving it value
        // semantics; we check for their existence as a proxy.
        if self.ctx.binder.global_augmentations.contains_key(name) {
            return true;
        }
        for lib_ctx in self.ctx.lib_contexts.iter() {
            if lib_ctx.binder.global_augmentations.contains_key(name) {
                return true;
            }
        }
        if let Some(ref all_binders) = self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder.global_augmentations.contains_key(name) {
                    return true;
                }
            }
        }

        false
    }

    pub(crate) fn current_file_is_module_for_umd_global_access(&self) -> bool {
        if self.ctx.binder.is_external_module() {
            return true;
        }

        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }

        let Some(source_file) = self.ctx.arena.source_files.get(self.ctx.current_file_idx) else {
            return false;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match stmt.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    let Some(var_stmt) = self.ctx.arena.get_variable(stmt) else {
                        continue;
                    };
                    for &list_idx in &var_stmt.declarations.nodes {
                        let Some(list_node) = self.ctx.arena.get(list_idx) else {
                            continue;
                        };
                        let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                            continue;
                        };
                        for &decl_idx in &var_list.declarations.nodes {
                            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node)
                            else {
                                continue;
                            };
                            if decl.initializer.is_some()
                                && self
                                    .get_require_module_specifier(decl.initializer)
                                    .is_some()
                            {
                                return true;
                            }
                        }
                    }
                }
                syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt) else {
                        continue;
                    };
                    if self
                        .get_require_module_specifier(expr_stmt.expression)
                        .is_some()
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    fn cross_file_global_value_type_by_name(
        &mut self,
        name: &str,
        include_js: bool,
    ) -> Option<TypeId> {
        let entries = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
            .cloned()?;
        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;

        for (file_idx, sym_id) in entries {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }

            let Some(arena) = all_arenas.get(file_idx).cloned() else {
                continue;
            };
            let Some(binder) = all_binders.get(file_idx).cloned() else {
                continue;
            };
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };
            if !include_js && is_js_file_name(&source_file.file_name) {
                continue;
            }

            let Some(symbol) = binder.get_symbol(sym_id) else {
                continue;
            };
            if symbol.escaped_name != name
                || (symbol.flags & symbol_flags::VALUE) == 0
                || symbol.is_umd_export
            {
                continue;
            }

            let candidate_decl = symbol
                .declarations
                .iter()
                .copied()
                .find(|&decl_idx| {
                    if !decl_idx.is_some() {
                        return false;
                    }
                    let Some(node) = arena.get(decl_idx) else {
                        return false;
                    };
                    matches!(
                        node.kind,
                        tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
                            | tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT
                            | tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                    )
                })
                .or_else(|| {
                    (symbol.value_declaration.is_some()
                        && arena.get(symbol.value_declaration).is_some())
                    .then_some(symbol.value_declaration)
                })
                .unwrap_or(NodeIndex::NONE);

            if !Self::enter_cross_arena_delegation() {
                continue;
            }

            let mut checker = Box::new(CheckerState::with_parent_cache(
                arena.as_ref(),
                binder.as_ref(),
                self.ctx.types,
                source_file.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self,
            ));
            checker.ctx.copy_cross_file_state_from(&self.ctx);
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            checker.ctx.current_file_idx = file_idx;
            checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
            checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
            checker
                .ctx
                .symbol_resolution_depth
                .set(self.ctx.symbol_resolution_depth.get());

            let candidate_type = if candidate_decl.is_some() {
                checker.type_of_value_declaration(candidate_decl)
            } else {
                checker.get_type_of_symbol(sym_id)
            };

            Self::leave_cross_arena_delegation();

            if !matches!(
                candidate_type,
                TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
            ) {
                return Some(candidate_type);
            }
        }

        None
    }

    fn non_js_cross_file_global_value_type_by_name(&mut self, name: &str) -> Option<TypeId> {
        self.cross_file_global_value_type_by_name(name, false)
    }

    pub(crate) fn preferred_non_js_cross_file_global_value_type(
        &mut self,
        name: &str,
        local_sym_id: SymbolId,
    ) -> Option<TypeId> {
        if self.ctx.binder.file_locals.get(name) != Some(local_sym_id) {
            return None;
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) {
            for &decl_idx in &symbol.declarations {
                if !decl_idx.is_some() {
                    continue;
                }
                let Some(source_file) = self.source_file_data_for_node(decl_idx) else {
                    continue;
                };
                if source_file.file_name == self.ctx.file_name
                    || is_js_file_name(&source_file.file_name)
                {
                    continue;
                }

                let candidate_type = self.type_of_value_declaration(decl_idx);
                if !matches!(
                    candidate_type,
                    TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
                ) {
                    return Some(candidate_type);
                }
            }
        }
        self.non_js_cross_file_global_value_type_by_name(name)
    }

    /// Returns `true` if `idx` is the name identifier inside an
    /// `export as namespace X` declaration.
    fn is_namespace_export_declaration_name(&self, idx: NodeIndex) -> bool {
        if let Some(ext) = self.ctx.arena.get_extended(idx)
            && ext.parent.is_some()
            && let Some(parent) = self.ctx.arena.get(ext.parent)
        {
            return parent.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION;
        }
        false
    }

    /// Returns `true` if `idx` is the expression identifier inside an
    /// `export = X` assignment. That reference is part of the UMD definition
    /// site and should not be treated as a module-side UMD global usage.
    fn is_export_assignment_expression_name(&self, idx: NodeIndex) -> bool {
        if let Some(ext) = self.ctx.arena.get_extended(idx)
            && ext.parent.is_some()
            && let Some(parent) = self.ctx.arena.get(ext.parent)
            && parent.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            && let Some(assign) = self.ctx.arena.get_export_assignment(parent)
        {
            return assign.expression == idx;
        }
        false
    }

    fn is_root_identifier_of_js_prototype_assignment(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let mut current = idx;
        let Some(first_parent_ext) = self.ctx.arena.get_extended(current) else {
            return false;
        };
        let first_parent = first_parent_ext.parent;
        let Some(first_parent_node) = self.ctx.arena.get(first_parent) else {
            return false;
        };
        if first_parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && first_parent_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }

        let Some(first_access) = self.ctx.arena.get_access_expr(first_parent_node) else {
            return false;
        };
        if first_access.expression != current
            || !self.access_member_is_named(first_access.name_or_argument, "prototype")
        {
            return false;
        }
        current = first_parent;

        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };

            if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                && access.expression == current
            {
                current = parent;
                continue;
            }

            return parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_binary_expr(parent_node)
                    .is_some_and(|binary| {
                        binary.left == current && self.is_assignment_operator(binary.operator_token)
                    });
        }
    }

    fn access_member_is_named(&self, idx: NodeIndex, expected: &str) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == expected;
        }

        if let Some(lit) = self.ctx.arena.get_literal(node) {
            return lit.text == expected;
        }

        false
    }

    pub(crate) fn source_file_has_value_import_binding_named(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        let mut current = idx;
        let mut guard = 0u32;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            guard += 1;
            if guard > 4096 {
                return false;
            }
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
            if let Some(node) = self.ctx.arena.get(current)
                && (node.kind == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
                    || node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK)
            {
                break;
            }
        }
        let Some(root) = self.ctx.arena.get(current) else {
            return false;
        };
        if root.kind != tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
            && root.kind != tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
        {
            return false;
        }

        for stmt_idx in self.ctx.arena.get_children(current) {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }
            let Some(import_decl) = self.ctx.arena.get_import_decl(stmt_node) else {
                continue;
            };
            // For import-equals declarations, is_type_only lives on ImportDeclData.
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                if import_decl.is_type_only {
                    continue;
                }
                if self
                    .ctx
                    .arena
                    .get_identifier_text(import_decl.import_clause)
                    == Some(name)
                {
                    // Check if the target module's export= is type-only
                    if let Some(module_specifier) = self.get_import_module_specifier(import_decl)
                        && self.is_module_export_equals_type_only(&module_specifier)
                    {
                        // type-only through export= chain — skip
                    } else {
                        return true;
                    }
                }
                continue;
            }
            // For regular import declarations, is_type_only lives on ImportClauseData,
            // NOT on ImportDeclData (which is always false for regular imports).
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                continue;
            };
            // Skip the entire import if the clause is type-only (`import type { ... }`)
            if clause.is_type_only {
                continue;
            }

            // Default import: `import Foo from ...`
            if clause.name.is_some()
                && self.ctx.arena.get_identifier_text(clause.name) == Some(name)
            {
                // Also check cross-file type-only chain for default imports.
                if let Some(module_specifier) = self.get_import_module_specifier(import_decl) {
                    if self.is_export_type_only_across_binders(&module_specifier, "default")
                        || self.is_module_export_equals_type_only(&module_specifier)
                    {
                        // type-only through export chain or export= chain — skip
                    } else {
                        return true;
                    }
                } else {
                    return true;
                }
            }

            // Named imports: `import { Foo } from ...`
            if clause.named_bindings.is_some()
                && let Some(named_bindings_node) = self.ctx.arena.get(clause.named_bindings)
                && (named_bindings_node.kind == tsz_parser::parser::syntax_kind_ext::NAMED_IMPORTS
                    || named_bindings_node.kind
                        == tsz_parser::parser::syntax_kind_ext::NAMESPACE_IMPORT)
                && let Some(named_imports) = self.ctx.arena.get_named_imports(named_bindings_node)
            {
                if named_bindings_node.kind == tsz_parser::parser::syntax_kind_ext::NAMESPACE_IMPORT
                {
                    if named_imports.name.is_some()
                        && self.ctx.arena.get_identifier_text(named_imports.name) == Some(name)
                    {
                        if let Some(module_specifier) =
                            self.get_import_module_specifier(import_decl)
                        {
                            if self.is_export_type_only_across_binders(&module_specifier, "*")
                                || self.is_module_export_equals_type_only(&module_specifier)
                            {
                                // type-only through export chain or export= chain — skip
                            } else {
                                return true;
                            }
                        } else {
                            return true;
                        }
                    }
                } else {
                    for &specifier_idx in &named_imports.elements.nodes {
                        let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                            continue;
                        };
                        let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                            continue;
                        };
                        // Skip individual type-only specifiers (`import { type Foo, Bar }`)
                        if specifier.is_type_only {
                            continue;
                        }
                        let local_name = specifier.name;
                        if local_name.is_none() {
                            continue;
                        }
                        if self.ctx.arena.get_identifier_text(local_name) == Some(name) {
                            // Also check whether this import's target is type-only through
                            // the export chain (e.g., `import { A } from './b'` where b.ts
                            // has `export type * from './a'`). If the target export is
                            // type-only, this import doesn't provide a runtime value binding.
                            if let Some(module_specifier) =
                                self.get_import_module_specifier(import_decl)
                            {
                                // Get the original export name (before any rename).
                                // For `import { Foo as Bar }`, the export name is "Foo".
                                let export_name = if specifier.property_name.is_some() {
                                    self.ctx
                                        .arena
                                        .get_identifier_text(specifier.property_name)
                                        .unwrap_or(name)
                                } else {
                                    name
                                };
                                if self.is_export_type_only_across_binders(
                                    &module_specifier,
                                    export_name,
                                ) {
                                    continue; // type-only through export chain — skip
                                }
                            }
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// For a merged interface+value symbol (e.g. `Promise`, `Symbol`), pick the
    /// best value declaration from the list. Prefers `VariableDeclaration` nodes
    /// (corresponding to `declare var X: XConstructor`) over interface or other
    /// declaration kinds, since those carry the constructor-side type.
    pub(crate) fn preferred_value_declaration(
        &self,
        sym_id: SymbolId,
        default_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> Option<NodeIndex> {
        // Among all declarations, prefer a VariableDeclaration with a type
        // annotation — this is the `declare var X: XConstructor` pattern that
        // gives us the constructor type for merged interface+value symbols.
        for &decl_idx in declarations {
            if decl_idx == default_decl || decl_idx.is_none() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                && let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
                && var_decl.type_annotation.is_some()
            {
                return Some(decl_idx);
            }
        }
        // Also check the default declaration itself
        if let Some(node) = self.ctx.arena.get(default_decl)
            && node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            return Some(default_decl);
        }
        if let Some(js_ctor_decl) =
            self.checked_js_constructor_value_declaration(sym_id, default_decl, declarations)
        {
            return Some(js_ctor_decl);
        }
        None
    }

    pub(crate) fn checked_js_constructor_value_declaration(
        &self,
        sym_id: SymbolId,
        default_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> Option<NodeIndex> {
        if self.declaration_is_checked_js_constructor_value_declaration(sym_id, default_decl) {
            return Some(default_decl);
        }

        declarations.iter().copied().find(|&decl_idx| {
            decl_idx != default_decl
                && self.declaration_is_checked_js_constructor_value_declaration(sym_id, decl_idx)
        })
    }

    pub(crate) fn declaration_is_checked_js_constructor_value_declaration(
        &self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        if decl_idx.is_none() {
            return false;
        }

        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            return arenas.iter().any(|arena| {
                self.arena_has_checked_js_constructor_value_declaration(arena.as_ref(), decl_idx)
            });
        }

        self.arena_has_checked_js_constructor_value_declaration(self.ctx.arena, decl_idx)
    }

    fn arena_has_checked_js_constructor_value_declaration(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        if !is_js_file_name(&source_file.file_name)
            || !should_resolve_jsdoc_for_file(
                &source_file.file_name,
                source_file.text.as_ref(),
                &self.ctx.compiler_options,
            )
        {
            return false;
        }

        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }

        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return false;
        };
        let Some(init_node) = arena.get(var_decl.initializer) else {
            return false;
        };

        init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
    }

    /// Extract the module specifier string from an import declaration.
    ///
    /// Given an `ImportDeclData`, resolves the `module_specifier` node index
    /// to a string literal text value (without quotes).
    fn get_import_module_specifier(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> Option<String> {
        let spec_node = self.ctx.arena.get(import_decl.module_specifier)?;
        let literal = self.ctx.arena.get_literal(spec_node)?;
        Some(literal.text.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_codes;

    /// TS1212 must fire when a strict-mode reserved word is used as an expression.
    /// In ESM (.ts files), strict mode is always on, so `var interface = 1; interface;`
    /// should emit TS1212 at the expression usage of `interface`.
    #[test]
    fn ts1212_expression_usage_of_strict_mode_reserved_word() {
        let codes = check_source_codes("var interface = 1;\ninterface;");
        assert!(
            codes.contains(&1212),
            "Expected TS1212 for expression usage of `interface`: {codes:?}"
        );
    }

    /// All strict-mode reserved words should trigger TS1212 at expression position.
    #[test]
    fn ts1212_all_reserved_words_in_expression() {
        for word in &[
            "implements",
            "interface",
            "let",
            "package",
            "private",
            "protected",
            "public",
            "static",
            "yield",
        ] {
            let source = format!("var {word} = 1;\n{word};");
            let codes = check_source_codes(&source);
            assert!(
                codes.contains(&1212),
                "Expected TS1212 for expression usage of `{word}`: {codes:?}"
            );
        }
    }

    /// Non-reserved identifiers should NOT get TS1212.
    #[test]
    fn no_ts1212_for_regular_identifiers() {
        let codes = check_source_codes("var foo = 1;\nfoo;");
        assert!(
            !codes.contains(&1212),
            "Should not emit TS1212 for regular identifier: {codes:?}"
        );
    }

    /// TS1361 must fire when a type-only import is used in a value position
    /// (object literal computed property name). Ensures that
    /// `source_file_has_value_import_binding_named` correctly checks
    /// `ImportClauseData::is_type_only` (not `ImportDeclData::is_type_only`,
    /// which is always false for regular import declarations).
    #[test]
    fn ts1361_type_only_import_in_value_computed_property() {
        let codes = check_source_codes(
            r#"
import type { onInit } from './hooks';
const o = { [onInit]: 0 };
"#,
        );
        assert!(
            codes.contains(&1361),
            "Expected TS1361 for type-only import used in object literal computed property: {codes:?}"
        );
    }

    /// TS1361 must NOT fire when a regular (non-type-only) import is used
    /// in value position. The value import binding shadows any type-only
    /// import of the same name.
    #[test]
    fn no_ts1361_for_regular_import_with_same_name() {
        let codes = check_source_codes(
            r#"
import { onInit } from './hooks';
const o = { [onInit]: 0 };
"#,
        );
        assert!(
            !codes.contains(&1361),
            "Should not emit TS1361 for regular (non-type-only) import: {codes:?}"
        );
    }

    /// When `import { type Foo }` is used, `Foo` is type-only per-specifier.
    /// Using `Foo` in a value position should emit TS1361.
    #[test]
    fn ts1361_respects_per_specifier_type_only() {
        let codes = check_source_codes(
            r#"
import { type Foo } from './hooks';
let x = Foo;
"#,
        );
        assert!(
            codes.contains(&1361),
            "Expected TS1361 for per-specifier type-only import used as value: {codes:?}"
        );
    }
}
