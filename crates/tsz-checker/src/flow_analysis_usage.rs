//! Flow-based definite assignment and declaration ordering checks.

use std::rc::Rc;

use crate::FlowAnalyzer;
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::definite_assignment::should_report_variable_use_before_assignment;
use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check flow-aware usage of a variable (definite assignment + type narrowing).
    ///
    /// This is the main entry point for flow analysis when variables are used.
    /// It combines two critical TypeScript features:
    /// 1. **Definite Assignment Analysis**: Catches use-before-assignment errors
    /// 2. **Type Narrowing**: Refines types based on control flow
    ///
    /// ## Definite Assignment Checking:
    /// - Block-scoped variables (let/const) without initializers are checked
    /// - Variables are tracked through all code paths
    /// - TS2454 error emitted if variable might not be assigned
    /// - Error: "Variable 'x' is used before being assigned"
    ///
    /// ## Type Narrowing:
    /// - If definitely assigned, applies flow-based type narrowing
    /// - typeof guards, discriminant checks, null checks refine types
    /// - Returns narrowed type for precise type checking
    ///
    /// ## Rule #42 Integration:
    /// - If inside a closure and variable is mutable (let/var): Returns declared type
    /// - If inside a closure and variable is const: Applies narrowing
    pub fn check_flow_usage(
        &mut self,
        idx: NodeIndex,
        declared_type: TypeId,
        sym_id: SymbolId,
    ) -> TypeId {
        use tracing::trace;

        trace!(?idx, ?declared_type, ?sym_id, "check_flow_usage called");

        // Flow narrowing is only meaningful for variable-like bindings.
        // Class/function/namespace symbols have stable declared types and
        // do not participate in definite-assignment analysis.
        if !self.symbol_participates_in_flow_analysis(sym_id) {
            trace!("Symbol does not participate in flow analysis, returning declared type");
            return declared_type;
        }

        // Const object/array literal bindings have a stable type shape and do not
        // benefit from control-flow narrowing. Skipping CFG traversal for these
        // bindings avoids O(N²) reference matching on large call-heavy files.
        if self.should_skip_flow_narrowing_for_const_literal_binding(sym_id) {
            return declared_type;
        }

        // Check definite assignment for block-scoped variables without initializers
        if should_report_variable_use_before_assignment(self, idx, declared_type, sym_id) {
            // Report TS2454 error: Variable used before assignment
            self.emit_definite_assignment_error(idx, sym_id);
            // Return declared type to avoid cascading errors
            trace!("Definite assignment error, returning declared type");
            return declared_type;
        }

        // Apply type narrowing based on control flow
        trace!("Applying flow narrowing");
        let result = self.apply_flow_narrowing(idx, declared_type);
        trace!(?result, "check_flow_usage result");
        result
    }

    fn symbol_participates_in_flow_analysis(&self, sym_id: SymbolId) -> bool {
        use tsz_binder::symbol_flags;

        self.ctx
            .binder
            .get_symbol(sym_id)
            .is_some_and(|symbol| (symbol.flags & symbol_flags::VARIABLE) != 0)
    }

    fn should_skip_flow_narrowing_for_const_literal_binding(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        let mut value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return false;
        }

        let mut decl_node = match self.ctx.arena.get(value_decl) {
            Some(node) => node,
            None => return false,
        };

        // Binder symbols can point at the identifier node for the declaration name.
        // Normalize to the enclosing VARIABLE_DECLARATION before checking const/init shape.
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(value_decl)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            value_decl = ext.parent;
            decl_node = parent_node;
        }

        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.is_const_variable_declaration(value_decl)
        {
            return false;
        }

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if var_decl.type_annotation.is_some() || var_decl.initializer.is_none() {
            return false;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
    }

    /// Emit TS2454 error for variable used before definite assignment.
    fn emit_definite_assignment_error(&mut self, idx: NodeIndex, sym_id: SymbolId) {
        // Get the location for error reporting and deduplication key
        let Some(node) = self.ctx.arena.get(idx) else {
            // If the node doesn't exist in the arena, we can't deduplicate by position
            // Skip error emission to avoid potential duplicates
            return;
        };

        let pos = node.pos;

        // Deduplicate: check if we've already emitted an error for this (node, symbol) pair
        let key = (pos, sym_id);
        if !self.ctx.emitted_ts2454_errors.insert(key) {
            // Already inserted - duplicate error, skip
            return;
        }

        // Get the variable name for the error message
        let name = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());

        // Get the location for error reporting
        let length = node.end - node.pos;

        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            pos,
            length,
            format!("Variable '{name}' is used before being assigned."),
            2454, // TS2454
        ));
    }

    /// Check if a node is within a parameter's default value initializer.
    /// This is used to detect `await` used in default parameter values (TS2524).
    pub(crate) fn is_in_default_parameter(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }

            // Check if parent is a parameter and we're in its initializer
            if let Some(parent_node) = self.ctx.arena.get(parent_idx) {
                if parent_node.kind == syntax_kind_ext::PARAMETER
                    && let Some(param) = self.ctx.arena.get_parameter(parent_node)
                {
                    // Check if current node is within the initializer
                    if param.initializer.is_some() {
                        let init_idx = param.initializer;
                        // Check if idx is within the initializer subtree
                        if self.is_node_within(idx, init_idx) {
                            return true;
                        }
                    }
                }
                // Stop at function/arrow boundaries - parameters are only at the top level
                if matches!(parent_node.kind,
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION ||
                         k == syntax_kind_ext::FUNCTION_EXPRESSION ||
                         k == syntax_kind_ext::ARROW_FUNCTION ||
                         k == syntax_kind_ext::METHOD_DECLARATION ||
                         k == syntax_kind_ext::CONSTRUCTOR ||
                         k == syntax_kind_ext::GET_ACCESSOR ||
                         k == syntax_kind_ext::SET_ACCESSOR
                ) {
                    return false;
                }
            }

            current = parent_idx;
        }
    }

    // =========================================================================
    // Definite Assignment Checking
    // =========================================================================

    /// Check if definite assignment checking should be skipped for a given type.
    /// TypeScript skips TS2454 when the declared type is `any`, `unknown`, or includes `undefined`.
    pub(crate) fn skip_definite_assignment_for_type(&self, declared_type: TypeId) -> bool {
        use tsz_solver::TypeId;
        use tsz_solver::type_contains_undefined;

        // Skip for any/unknown/error - these types allow uninitialized usage
        if declared_type == TypeId::ANY
            || declared_type == TypeId::UNKNOWN
            || declared_type == TypeId::ERROR
        {
            return true;
        }

        // Skip if the type includes undefined or void (uninitialized variables are undefined)
        type_contains_undefined(self.ctx.types, declared_type)
    }

    /// - Not in ambient contexts
    /// - Not in type-only positions
    pub(crate) fn should_check_definite_assignment(
        &mut self,
        sym_id: SymbolId,
        idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::node::NodeAccess;
        use tsz_scanner::SyntaxKind;

        // TS2454 is only emitted under strictNullChecks (matches tsc behavior)
        if !self.ctx.strict_null_checks() {
            return false;
        }

        // Skip definite assignment check if this identifier is a for-in/for-of
        // initializer — it's an assignment target, not a usage.
        // e.g., `let x: number; for (x of items) { ... }` — the `x` in `for (x of ...)`
        // is being written to, not read from.
        if self.is_for_in_of_initializer(idx) {
            return false;
        }

        // Skip definite assignment check if this identifier is an assignment target
        // in a destructuring assignment — it's being written to, not read.
        // e.g., `let x: string; [x] = items;` — the `x` is being assigned to.
        if self.is_destructuring_assignment_target(idx) {
            return false;
        }

        // Get the symbol
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check both block-scoped (let/const) and function-scoped (var) variables.
        // Parameters are excluded downstream (PARAMETER nodes ≠ VARIABLE_DECLARATION).
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        // Get the value declaration
        let decl_id = symbol.value_declaration;
        if decl_id.is_none() {
            return false;
        }

        // Get the declaration node
        let Some(decl_node) = self.ctx.arena.get(decl_id) else {
            return false;
        };

        // Check if it's a variable declaration
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }

        // Get the variable declaration data
        let Some(var_data) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };

        // If there's an initializer, skip definite assignment check — unless the variable
        // is `var` (function-scoped) and the usage is before the declaration in source
        // order.  `var` hoists the binding but NOT the initializer, so at the usage
        // point the variable is `undefined`.  Block-scoped variables (let/const) don't
        // need this: TDZ checks handle pre-declaration use separately.
        if var_data.initializer.is_some() {
            let is_function_scoped =
                symbol.flags & tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE != 0;
            if !is_function_scoped {
                return false;
            }
            // For `var` with initializer, only proceed when usage is before the
            // declaration in source order (the initializer hasn't executed yet).
            let usage_before_decl = self
                .ctx
                .arena
                .get(idx)
                .and_then(|usage_node| {
                    self.ctx
                        .arena
                        .get(decl_id)
                        .map(|decl_node| usage_node.pos < decl_node.pos)
                })
                .unwrap_or(false);
            if !usage_before_decl {
                return false;
            }
        }

        // If there's a definite assignment assertion (!), skip check
        if var_data.exclamation_token {
            return false;
        }

        // If the variable is declared in a for-in or for-of loop header,
        // it's assigned by the loop iteration itself - but only when usage is at or after the loop.
        // A usage BEFORE the loop in source order (e.g. `v; for (var v of [0]) {}`) must still
        // be checked for definite assignment.
        if let Some(decl_list_info) = self.ctx.arena.node_info(decl_id) {
            let decl_list_idx = decl_list_info.parent;
            if let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx)
                && decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(for_info) = self.ctx.arena.node_info(decl_list_idx)
            {
                let for_idx = for_info.parent;
                if let Some(for_node) = self.ctx.arena.get(for_idx)
                    && (for_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                        || for_node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
                {
                    // Only skip the check if the usage is at or after the start of the loop.
                    // If the usage precedes the loop in source order, fall through to DAA.
                    if let Some(usage_node) = self.ctx.arena.get(idx) {
                        if usage_node.pos >= for_node.pos {
                            return false;
                        }
                        // Usage is before the loop - continue to definite assignment check
                    } else {
                        return false;
                    }
                }
            }
        }

        // For source-file globals, skip TS2454 when the usage occurs inside a
        // function-like body. The variable may be assigned before invocation.
        if self.is_source_file_global_var_decl(decl_id) && self.is_inside_function_like(idx) {
            return false;
        }

        // For namespace-scoped variables, skip TS2454 when the usage is inside
        // a nested namespace (MODULE_DECLARATION) relative to the declaration.
        // Flow analysis can't cross namespace boundaries, and the variable may
        // be assigned in the outer namespace before the inner namespace executes.
        // Same-namespace usage still gets TS2454 (flow analysis works within a scope).
        if self.is_usage_in_nested_namespace_from_decl(decl_id, idx) {
            return false;
        }

        // Walk up the parent chain to check:
        // 1. Skip definite assignment checks in ambient declarations (declare const/let)
        // 2. Anchor checks to a function-like or source-file container
        let mut current = decl_id;
        let mut found_container_scope = false;
        for _ in 0..50 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                break;
            };
            if let Some(node) = self.ctx.arena.get(current) {
                // Check for ambient declarations
                if (node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                    || node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST)
                    && let Some(var_data) = self.ctx.arena.get_variable(node)
                    && let Some(mods) = &var_data.modifiers
                {
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                            && mod_node.kind == SyntaxKind::DeclareKeyword as u16
                        {
                            return false;
                        }
                    }
                }

                // Check if we're inside a function-like or source-file container scope
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::GET_ACCESSOR
                    || node.kind == syntax_kind_ext::SET_ACCESSOR
                    || node.kind == syntax_kind_ext::SOURCE_FILE
                {
                    found_container_scope = true;
                    break;
                }
            }

            current = info.parent;
            if current.is_none() {
                break;
            }
        }

        // Only check definite assignment when we can anchor to a container scope.
        found_container_scope
    }

    fn is_source_file_global_var_decl(&self, decl_id: NodeIndex) -> bool {
        let Some(info) = self.ctx.arena.node_info(decl_id) else {
            return false;
        };
        let mut current = info.parent;
        for _ in 0..50 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::SOURCE_FILE {
                return true;
            }
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::METHOD_DECLARATION
                || node.kind == syntax_kind_ext::CONSTRUCTOR
                || node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return false;
            }
            let Some(next) = self.ctx.arena.node_info(current).map(|n| n.parent) else {
                return false;
            };
            current = next;
            if current.is_none() {
                return false;
            }
        }
        false
    }

    fn is_inside_function_like(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..50 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                return false;
            };
            current = info.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::METHOD_DECLARATION
                || node.kind == syntax_kind_ext::CONSTRUCTOR
                || node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return true;
            }
            if node.kind == syntax_kind_ext::SOURCE_FILE {
                return false;
            }
        }
        false
    }

    /// Check if a usage crosses a namespace boundary relative to its declaration.
    /// Walk up from the usage node; if we encounter a `MODULE_DECLARATION` before
    /// reaching the node that contains the declaration, the usage is in a nested
    /// namespace and TS2454 should be suppressed (flow graph doesn't span across
    /// namespace boundaries).
    fn is_usage_in_nested_namespace_from_decl(
        &self,
        decl_id: NodeIndex,
        usage_idx: NodeIndex,
    ) -> bool {
        let Some(decl_node) = self.ctx.arena.get(decl_id) else {
            return false;
        };
        let decl_pos = decl_node.pos;
        let decl_end = decl_node.end;

        let mut current = usage_idx;
        for _ in 0..50 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                break;
            };
            current = info.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            // If this node's span contains the declaration, we've reached the
            // common container — no namespace boundary between usage and decl.
            if node.pos <= decl_pos && node.end >= decl_end {
                return false;
            }
            // Hit a MODULE_DECLARATION before reaching the declaration's container:
            // usage is in a nested namespace.
            if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return true;
            }
            if current.is_none() {
                break;
            }
        }
        false
    }

    /// Check if a node is a for-in/for-of initializer (assignment target).
    /// For `for (x of items)`, the identifier `x` is the initializer and is
    /// being assigned to, not read from.
    fn is_for_in_of_initializer(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::node::NodeAccess;

        let Some(info) = self.ctx.arena.node_info(idx) else {
            return false;
        };
        let parent = info.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if (parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
            && let Some(for_data) = self.ctx.arena.get_for_in_of(parent_node)
            && for_data.initializer == idx
        {
            return true;
        }
        false
    }

    /// Check if an identifier is an assignment target in a destructuring assignment.
    /// e.g., `[x] = a` or `({x} = a)` — the `x` is being written to, not read.
    fn is_destructuring_assignment_target(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..10 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                return false;
            };
            let parent = info.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            match parent_node.kind {
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::SPREAD_ELEMENT
                    || k == syntax_kind_ext::SPREAD_ASSIGNMENT
                    || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT =>
                {
                    current = parent;
                }
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    // Check this is the LHS of a simple assignment (=)
                    if let Some(bin) = self.ctx.arena.get_binary_expr(parent_node)
                        && bin.operator_token == SyntaxKind::EqualsToken as u16
                        && bin.left == current
                    {
                        return true;
                    }
                    return false;
                }
                k if k == syntax_kind_ext::FOR_IN_STATEMENT
                    || k == syntax_kind_ext::FOR_OF_STATEMENT =>
                {
                    if let Some(for_node) = self.ctx.arena.get_for_in_of(parent_node)
                        && for_node.initializer == current
                    {
                        return true;
                    }
                    return false;
                }
                _ => return false,
            }
        }
        false
    }

    /// Check if a variable is definitely assigned at a given point.
    ///
    /// This performs flow-sensitive analysis to determine if a variable
    /// has been assigned on all code paths leading to the usage point.
    pub(crate) fn is_definitely_assigned_at(&self, idx: NodeIndex) -> bool {
        // Get the flow node for this identifier usage
        let flow_node = match self.ctx.binder.get_node_flow(idx) {
            Some(flow) => flow,
            None => return true, // No flow info - assume assigned to avoid false positives
        };

        // Create a flow analyzer and check definite assignment
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
        .with_type_environment(Rc::clone(&self.ctx.type_environment));

        analyzer.is_definitely_assigned(idx, flow_node)
    }

    // =========================================================================
    // Temporal Dead Zone (TDZ) Checking
    // =========================================================================

    /// Check if a variable is used before its declaration in a static block.
    ///
    /// This detects Temporal Dead Zone (TDZ) violations where a block-scoped variable
    /// is accessed inside a class static block before it has been declared in the source.
    ///
    /// # Example
    /// ```typescript
    /// class C {
    ///   static {
    ///     console.log(x); // Error: x used before declaration
    ///   }
    /// }
    /// let x = 1;
    /// ```
    pub(crate) fn is_variable_used_before_declaration_in_static_block(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        // var and function are hoisted, so they don't have TDZ issues in this context.
        // Imports (ALIAS) are also hoisted or handled differently.
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE
                | symbol_flags::CLASS
                | symbol_flags::REGULAR_ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // Skip cross-file symbols — TDZ position comparison only valid within same file
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return false;
        }

        // 3. Get the declaration node
        // Prefer value_declaration, fall back to first declaration
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        // We ensure both nodes exist in the current arena
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        // If usage is after declaration, it's valid
        if usage_node.pos >= decl_node.pos {
            return false;
        }

        // 5. Check if usage is inside a static block
        // Use find_enclosing_static_block which walks up the AST and stops at function boundaries.
        // This ensures we only catch immediate usage, not usage inside a closure/function
        // defined within the static block (which would execute later).
        if self.find_enclosing_static_block(usage_idx).is_some() {
            return true;
        }

        false
    }

    /// Check if a variable is used before its declaration in a computed property.
    ///
    /// Computed property names are evaluated before the property declaration,
    /// creating a TDZ for the class being declared.
    pub(crate) fn is_variable_used_before_declaration_in_computed_property(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE
                | symbol_flags::CLASS
                | symbol_flags::REGULAR_ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // Skip cross-file symbols — TDZ position comparison only valid within same file
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return false;
        }

        // 3. Get the declaration node
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if usage_node.pos >= decl_node.pos {
            return false;
        }

        // 5. Check if usage is inside a computed property name
        if self.find_enclosing_computed_property(usage_idx).is_some() {
            return true;
        }

        false
    }

    /// Check if a variable is used before its declaration in a heritage clause.
    ///
    /// Heritage clauses (extends, implements) are evaluated before the class body,
    /// creating a TDZ for the class being declared.
    pub(crate) fn is_variable_used_before_declaration_in_heritage_clause(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE
                | symbol_flags::CLASS
                | symbol_flags::REGULAR_ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // Skip TDZ check for type-only contexts (interface extends, type parameters, etc.)
        // Types are resolved at compile-time, so they don't have temporal dead zones.
        if self.is_in_type_only_context(usage_idx) {
            return false;
        }

        // Skip cross-file symbols — TDZ position comparison only makes sense
        // within the same file.
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return false;
        }

        // 3. Get the declaration node
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if usage_node.pos >= decl_node.pos {
            return false;
        }

        // 5. Check if usage is inside a heritage clause (extends/implements)
        if self.find_enclosing_heritage_clause(usage_idx).is_some() {
            return true;
        }

        false
    }

    /// TS2448/TS2449/TS2450: Check if a block-scoped declaration (class, enum,
    /// let/const) is used before its declaration in immediately executing code
    /// (not inside a function/method body).
    pub(crate) fn is_class_or_enum_used_before_declaration(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // Applies to block-scoped declarations: class, enum, let/const
        let is_block_scoped = (symbol.flags
            & (symbol_flags::CLASS
                | symbol_flags::REGULAR_ENUM
                | symbol_flags::BLOCK_SCOPED_VARIABLE))
            != 0;
        if !is_block_scoped {
            return false;
        }

        // Skip TDZ check for type-only contexts (type annotations, typeof in types, etc.)
        // Types are resolved at compile-time, so they don't have temporal dead zones.
        if self.is_in_type_only_context(usage_idx) {
            return false;
        }

        // Skip check for cross-file symbols (imported from another file).
        // Position comparison only makes sense within the same file.
        if symbol.import_module.is_some() {
            return false;
        }
        let is_cross_file = symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32;

        if is_cross_file
            && (self.ctx.current_file_idx as u32) > symbol.decl_file_idx {
                return false;
            }

        // In multi-file mode, symbol declarations may reference nodes in another
        // file's arena.  `self.ctx.arena` only contains the *current* file, so
        // looking up the declaration index would yield an unrelated node whose
        // position comparison is meaningless.  Detect this by verifying that the
        // node found at the declaration index really IS a class / enum / variable
        // declaration — if it isn't, the index came from a different arena.
        let is_multi_file = self.ctx.all_arenas.is_some();

        // Get the declaration position
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };

        let mut decl_node_opt = self.ctx.arena.get(decl_idx);
        let mut decl_arena = self.ctx.arena;

        if is_cross_file
            && let Some(arenas) = self.ctx.all_arenas.as_ref()
                && let Some(arena) = arenas.get(symbol.decl_file_idx as usize) {
                    decl_node_opt = arena.get(decl_idx);
                    decl_arena = arena.as_ref();
                }

        let Some(decl_node) = decl_node_opt else {
            return false;
        };

        // In multi-file mode, validate the declaration node kind matches the
        // symbol.  A mismatch means the node index is from a different file's
        // arena and should not be compared.
        if is_multi_file && !is_cross_file {
            let is_class = symbol.flags & symbol_flags::CLASS != 0;
            let is_enum = symbol.flags & symbol_flags::REGULAR_ENUM != 0;
            let is_var = symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0;
            let kind_ok = (is_class
                && (decl_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || decl_node.kind == syntax_kind_ext::CLASS_EXPRESSION))
                || (is_enum && decl_node.kind == syntax_kind_ext::ENUM_DECLARATION)
                || (is_var
                    && (decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                        || decl_node.kind == syntax_kind_ext::PARAMETER));
            if !kind_ok {
                return false;
            }
        }

        // Skip ambient declarations — `declare class`/`declare enum` are type-level
        // and have no TDZ. In multi-file mode, search all arenas since decl_idx may
        // point to a node in another file's arena.
        if is_cross_file {
            if let Some(class) = decl_arena.get_class(decl_node)
                && self.has_declare_modifier_in_arena(decl_arena, &class.modifiers)
            {
                return false;
            }
            if let Some(enum_decl) = decl_arena.get_enum(decl_node)
                && self.has_declare_modifier_in_arena(decl_arena, &enum_decl.modifiers)
            {
                return false;
            }
        } else if self.is_ambient_declaration(decl_idx) {
            return false;
        }

        // Only flag if usage is before declaration in source order
        if !is_cross_file && usage_node.pos >= decl_node.pos {
            return false;
        }

        // Find the declaration's enclosing function-like container (or source file).
        // This is the scope that "owns" both the declaration and (potentially) the usage.
        let decl_container = if is_cross_file {
            None // Walk up to source file
        } else {
            Some(self.find_enclosing_function_or_source_file(decl_idx))
        };

        // Walk up from usage: if we hit a function-like boundary BEFORE reaching
        // the declaration's container, the usage is in deferred code (a nested
        // function/arrow/method) and is NOT a TDZ violation.
        // If we reach the declaration's container without crossing a function
        // boundary, the usage executes immediately and IS a violation.
        let mut current = usage_idx;
        while current.is_some() {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            // If we reached the declaration container, stop - same scope means TDZ
            if Some(current) == decl_container {
                break;
            }
            // If we reach a function-like boundary before the decl container,
            // the usage is deferred and not a TDZ violation.
            // Exception: IIFEs (immediately invoked function expressions) execute
            // immediately, so they ARE TDZ violations.
            if node.is_function_like() && !self.is_immediately_invoked(current) {
                return false;
            }
            // IIFE - continue walking up, this function executes immediately
            // Non-static class property initializers run during constructor execution,
            // which is deferred — not a TDZ violation for class declarations.
            if node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.ctx.arena.get_property_decl(node)
                && !self.has_static_modifier(&prop.modifiers)
            {
                return false;
            }
            // Export assignments (`export = X` / `export default X`) are not TDZ
            // violations: the compiler reorders them after all declarations, so
            // the referenced class/variable is initialized by the time the export
            // binding is created.
            if node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                return false;
            }
            // Stop at source file
            if node.kind == syntax_kind_ext::SOURCE_FILE {
                break;
            }
            // Walk to parent
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        true
    }

    /// Check if a modifier list in a specific arena contains the `declare` keyword.
    /// Used in multi-file mode where `self.ctx.arena` may not be the declaration's arena.
    pub(crate) fn has_declare_modifier_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = arena.get(mod_idx)
                    && mod_node.kind == tsz_scanner::SyntaxKind::DeclareKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a node is in a type-only context (type annotation, type query, heritage clause).
    /// References in type-only positions don't need TDZ checks because types are
    /// resolved at compile-time, not runtime.
    fn is_in_type_only_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                return false;
            };

            // Type node kinds indicate we're in a type-only context
            match parent_node.kind {
                // Core type nodes
                syntax_kind_ext::TYPE_PREDICATE
                | syntax_kind_ext::TYPE_REFERENCE
                | syntax_kind_ext::FUNCTION_TYPE
                | syntax_kind_ext::CONSTRUCTOR_TYPE
                | syntax_kind_ext::TYPE_QUERY // typeof T in type position
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::ARRAY_TYPE
                | syntax_kind_ext::TUPLE_TYPE
                | syntax_kind_ext::OPTIONAL_TYPE
                | syntax_kind_ext::REST_TYPE
                | syntax_kind_ext::UNION_TYPE
                | syntax_kind_ext::INTERSECTION_TYPE
                | syntax_kind_ext::CONDITIONAL_TYPE
                | syntax_kind_ext::INFER_TYPE
                | syntax_kind_ext::PARENTHESIZED_TYPE
                | syntax_kind_ext::THIS_TYPE
                | syntax_kind_ext::TYPE_OPERATOR
                | syntax_kind_ext::INDEXED_ACCESS_TYPE
                | syntax_kind_ext::MAPPED_TYPE
                | syntax_kind_ext::LITERAL_TYPE
                | syntax_kind_ext::NAMED_TUPLE_MEMBER
                | syntax_kind_ext::TEMPLATE_LITERAL_TYPE
                | syntax_kind_ext::IMPORT_TYPE
                | syntax_kind_ext::HERITAGE_CLAUSE
                | syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => return true,

                // Stop at boundaries that separate type from value context
                syntax_kind_ext::TYPE_OF_EXPRESSION // typeof x in value position
                | syntax_kind_ext::SOURCE_FILE => return false,

                _ => {
                    // Continue walking up
                    current = ext.parent;
                }
            }
        }
        false
    }

    /// Check if a function-like node is immediately invoked (IIFE pattern).
    /// Detects patterns like `(() => expr)()` and `(function() {})()`.
    fn is_immediately_invoked(&self, func_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up through parenthesized expressions to find if the function
        // is the callee of a call expression.
        let mut current = func_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                // Continue walking up through parens: ((fn))()
                current = ext.parent;
                continue;
            }
            if parent_node.kind == syntax_kind_ext::CALL_EXPRESSION {
                // Check that the function is the callee (expression), not an argument
                if let Some(call_data) = self.ctx.arena.get_call_expr(parent_node) {
                    return call_data.expression == current;
                }
            }
            return false;
        }
    }

    /// Find the enclosing function-like node or source file for a given node.
    fn find_enclosing_function_or_source_file(&self, idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        while current.is_some() {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.is_function_like() || node.kind == syntax_kind_ext::SOURCE_FILE {
                return current;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        current
    }
}
