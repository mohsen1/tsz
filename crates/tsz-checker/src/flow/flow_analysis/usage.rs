//! Flow-based definite assignment and declaration ordering checks.

use crate::FlowAnalyzer;
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

        // Apply type narrowing based on control flow FIRST.
        // This is needed to handle typeof guards correctly: `typeof x === "string"`
        // narrows x to `string` in the true branch, and the narrowed type means
        // x has a definite value — TS2454 should not fire in narrowed branches.
        trace!("Applying flow narrowing");
        let mut narrowed_type = self.apply_flow_narrowing(idx, declared_type);
        if declared_type == TypeId::ANY && self.is_control_flow_typed_any_symbol(sym_id) {
            let is_assigned = self.is_definitely_assigned_at_with_symbol(idx, Some(sym_id));
            if is_assigned {
                let evolved_type = self.apply_flow_narrowing_with_initial_type(
                    idx,
                    declared_type,
                    Some(TypeId::NEVER),
                );
                if evolved_type != TypeId::NEVER && evolved_type != TypeId::ERROR {
                    narrowed_type = evolved_type;
                }
            } else if self.is_same_function_scope_as_declaration(idx, sym_id)
                && !self.is_ambient_var_declaration(sym_id)
                && !self.is_in_assignment_target_position(idx)
            {
                // For control-flow-typed `any` variables (e.g., `var p;`) that are
                // NOT definitely assigned at the usage point, the runtime value is
                // `undefined` (var hoisting initializes to undefined). tsc uses
                // `undefined` as the initial type for such variables in its control
                // flow analysis. This causes downstream diagnostics like TS18048
                // ("'p' is possibly 'undefined'") when `p` is used in comparisons.
                //
                // Guards:
                // - Same function scope: cross-scope captures get TS7005/TS7034 instead
                // - Not ambient: `declare var` has no runtime initialization
                // - Not assignment target: destructuring/for-of targets are written, not read
                let evolved_type = self.apply_flow_narrowing_with_initial_type(
                    idx,
                    declared_type,
                    Some(TypeId::UNDEFINED),
                );
                if evolved_type != TypeId::ERROR && evolved_type != TypeId::ANY {
                    narrowed_type = evolved_type;
                }
            }
        }
        trace!(?narrowed_type, "flow narrowing result");

        // When flow analysis narrows to a type that is NOT assignable to the
        // declared type, the narrowing came from an invalid assignment (e.g.,
        // `var x: string; x = 0;` or a duplicate `var` declaration with
        // incompatible type).  tsc keeps the declared type in this situation.
        // Mark the node as flow-narrowed so the second narrowing pass in
        // `get_type_of_node` doesn't re-apply the invalid narrowing.
        //
        // IMPORTANT: Use `is_assignable_to_no_weak_checks` here to match
        // tsc's `isTypeAssignableTo` behavior. tsc does NOT include the weak
        // type check (TS2559) in this guard — the weak type check is only
        // applied at specific diagnostic sites (variable declarations,
        // argument passing, return statements). Without this, instanceof
        // narrowing from an interface to a class with no common properties
        // would be incorrectly rejected.
        let generic_narrowing_shape = self.contains_type_parameters_cached(declared_type)
            || self.contains_type_parameters_cached(narrowed_type);
        if narrowed_type != declared_type
            && narrowed_type != TypeId::ERROR
            && declared_type != TypeId::ANY
            && declared_type != TypeId::UNKNOWN
            && !generic_narrowing_shape
            && !self.is_assignable_to_no_weak_checks(narrowed_type, declared_type)
        {
            trace!("Flow narrowed to incompatible type, keeping declared type");
            self.ctx.flow_narrowed_nodes.insert(idx.0);
            return declared_type;
        }

        // Check definite assignment for block-scoped variables without initializers.
        // TS2454 is checked INDEPENDENTLY of narrowing. When a variable is used
        // before assignment, we emit TS2454 and return the declared type (not the
        // narrowed type). The definite assignment analysis itself handles typeof/
        // instanceof true branches — those are treated as proof that the variable
        // has a value, so TS2454 is suppressed in those branches.
        if should_report_variable_use_before_assignment(self, idx, declared_type, sym_id) {
            // Report TS2454 error: Variable used before assignment
            self.emit_definite_assignment_error(idx, sym_id);
            // Mark this node so `get_type_of_node` won't re-narrow via the second
            // flow-narrowing pass. Without this, the declared type returned here
            // gets overridden with the narrowed type, hiding TS2322 mismatches.
            self.ctx.daa_error_nodes.insert(idx.0);
            // For control-flow-typed `any` symbols (variables with no type
            // annotation like `var a;`), tsc uses `undefined` as the expression
            // type when TS2454 fires. This causes downstream type errors to
            // cascade (e.g., TS2345 "Argument of type 'undefined'...").
            // For explicitly-typed variables, the declared type is used.
            if declared_type == TypeId::ANY && self.is_control_flow_typed_any_symbol(sym_id) {
                trace!("Definite assignment error for implicit any, returning undefined");
                return TypeId::UNDEFINED;
            }
            trace!("Definite assignment error, returning declared type");
            return declared_type;
        }

        // Mark `any`-declared nodes so `get_type_of_node` won't apply a second
        // round of flow narrowing.  Double-narrowing corrupts `any` types:
        // `any` → `string` (typeof), then re-narrowing `string` through an
        // instanceof guard produces `string & Object` instead of `string`.
        // Only `any` is affected because its narrowing semantics differ from
        // other types (instanceof returns `any` unchanged, but narrowing `string`
        // through instanceof produces an intersection).
        if declared_type == TypeId::ANY && narrowed_type != declared_type {
            self.ctx.flow_narrowed_nodes.insert(idx.0);
        }

        trace!(?narrowed_type, "check_flow_usage result");
        narrowed_type
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

    fn is_control_flow_typed_any_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let mut value_decl = symbol.value_declaration;
        let Some(mut decl_node) = self.ctx.arena.get(value_decl) else {
            return false;
        };

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
            || self.is_const_variable_declaration(value_decl)
        {
            return false;
        }
        // Ambient declarations (`declare var x;`) have no runtime initialization —
        // they declare a type, not a control-flow-tracked binding.  The `any` type
        // from an ambient var should NOT be narrowed to `undefined` via flow analysis.
        if self.is_ambient_declaration(value_decl) {
            return false;
        }
        if let Some(ext) = self.ctx.arena.get_extended(value_decl)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::CATCH_CLAUSE
        {
            return false;
        }

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if var_decl.type_annotation.is_some()
            || self
                .ctx
                .arena
                .get(var_decl.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .is_none()
        {
            return false;
        }

        var_decl.initializer.is_none()
            || self
                .nullish_initializer_type(var_decl.initializer)
                .is_some()
    }

    fn nullish_initializer_type(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.ctx.arena.skip_parenthesized(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::NullKeyword as u16 {
            return Some(TypeId::NULL);
        }
        if node.kind == SyntaxKind::UndefinedKeyword as u16 {
            return Some(TypeId::UNDEFINED);
        }
        self.ctx
            .arena
            .get_identifier(node)
            .filter(|ident| ident.escaped_text == "undefined")
            .map(|_| TypeId::UNDEFINED)
    }

    /// Check if the symbol's declaration is ambient (e.g., `declare var Foo;`).
    fn is_ambient_var_declaration(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let mut value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return false;
        }
        if let Some(node) = self.ctx.arena.get(value_decl)
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(value_decl)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            value_decl = ext.parent;
        }
        self.ctx.is_ambient_declaration(value_decl)
    }

    /// Check if an identifier is in an assignment target position (LHS of `=`,
    /// for-of/for-in initializer, or destructuring assignment target).
    fn is_in_assignment_target_position(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent_node.kind {
                syntax_kind_ext::FOR_OF_STATEMENT | syntax_kind_ext::FOR_IN_STATEMENT => {
                    if let Some(data) = self.ctx.arena.get_for_in_of(parent_node) {
                        return current == data.initializer;
                    }
                    return false;
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(data) = self.ctx.arena.get_binary_expr(parent_node) {
                        if data.operator_token >= SyntaxKind::EqualsToken as u16
                            && data.operator_token <= SyntaxKind::CaretEqualsToken as u16
                        {
                            return current == data.left;
                        }
                    }
                    return false;
                }
                syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                | syntax_kind_ext::SPREAD_ELEMENT
                | syntax_kind_ext::SPREAD_ASSIGNMENT
                | syntax_kind_ext::PROPERTY_ASSIGNMENT
                | syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                | syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    current = parent_idx;
                    continue;
                }
                _ => return false,
            }
        }
        false
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

        self.error_at_node(
            idx,
            &format!("Variable '{name}' is used before being assigned."),
            2454, // TS2454
        );

        // Also buffer for deferred re-emission. check_flow_usage can run inside
        // speculative call-checker contexts (generic inference, overload probing)
        // that truncate diagnostics on rollback. The deferred buffer survives
        // rollback; at the end of check_source_file we re-emit any TS2454 that
        // was lost.
        self.ctx.deferred_ts2454_errors.push((idx, sym_id));
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
                if parent_node.is_function_like() {
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
    /// It also skips TS2454 entirely when `strictNullChecks` is disabled, because
    /// without strict null checks all types implicitly include `undefined`.
    pub(crate) fn skip_definite_assignment_for_type(&self, declared_type: TypeId) -> bool {
        use tsz_solver::TypeId;
        use tsz_solver::type_contains_undefined;

        // tsc gates TS2454 on strictNullChecks. Without it, every type implicitly
        // includes undefined/null, so an uninitialized variable is always valid.
        if !self.ctx.strict_null_checks() {
            return true;
        }

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

        // tsc 6.0: TS2454 fires regardless of strictNullChecks for variables
        // with type annotations used before assignment.

        // Skip definite assignment check for identifiers in type-query position
        // (e.g., `typeof x` in `let b: typeof x`). typeof in type position is a
        // compile-time type query that doesn't read the variable's value at runtime,
        // so TS2454 should not fire even if the variable has no initializer.
        if self.is_in_type_query_position(idx) {
            return false;
        }

        // `x!` is an assertion site — tsc treats this as an explicit developer
        // assertion that the variable has a value, so TS2454 does not fire.
        if self.is_non_null_assertion_operand(idx) {
            return false;
        }

        // Skip definite assignment check if this identifier is a for-in/for-of
        // initializer — it's an assignment target, not a usage.
        // e.g., `let x: number; for (x of items) { ... }` — the `x` in `for (x of ...)`
        // is being written to, not read from.
        if self.is_for_in_of_initializer(idx) {
            return false;
        }

        // Class computed property names don't trigger TS2454 in tsc.
        // Object literal computed property names DO trigger TS2454.
        // Check if we're in a computed property name inside a class body.
        if let Some(computed_idx) = self.find_enclosing_computed_property(idx)
            && self.is_class_member_computed_property(computed_idx)
        {
            return false;
        }

        // Class field initializers execute later (instance construction), so a
        // variable read there is not a same-point definite-assignment use.
        if self.is_inside_class_property_initializer(idx) {
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
            if std::env::var_os("TSZ_DAA_DEBUG").is_some() {
                tracing::warn!(
                    "[DAA] should_check_definite_assignment: no symbol for {:?}",
                    sym_id
                );
            }
            return false;
        };

        // Flow analysis operates within a single file's AST.
        // We cannot prove assignment across files, so we assume it was assigned.
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return false;
        }

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

        // Skip declarations that live in a different arena (for example lib
        // globals such as `Symbol`). Their flow is not modeled in the current
        // file, and NodeIndex collisions can otherwise manufacture bogus TS2454s.
        if let Some(decl_arena) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(sym_id, decl_id))
            .and_then(|arenas| arenas.first())
            && !std::ptr::eq(decl_arena.as_ref(), self.ctx.arena)
        {
            return false;
        }

        // Get the declaration node
        let Some(decl_node) = self.ctx.arena.get(decl_id) else {
            return false;
        };

        let mut decl_node = decl_node;
        let mut decl_id_to_check = decl_id;
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(info) = self.ctx.arena.node_info(decl_id)
            && let Some(parent) = self.ctx.arena.get(info.parent)
        {
            decl_node = parent;
            decl_id_to_check = info.parent;
        }

        // If the declaration is a binding element that is ultimately a parameter,
        // we should not perform definite assignment checking. Parameters are
        // always definitely assigned.
        if decl_node.kind == syntax_kind_ext::BINDING_ELEMENT {
            let mut current = decl_id_to_check;
            for _ in 0..10 {
                if let Some(info) = self.ctx.arena.node_info(current) {
                    let parent = info.parent;
                    if let Some(parent_node) = self.ctx.arena.get(parent)
                        && parent_node.kind == syntax_kind_ext::PARAMETER
                    {
                        return false;
                    }
                    current = parent;
                } else {
                    break;
                }
            }
        }
        let (has_initializer, has_exclamation) =
            if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                let Some(var_data) = self.ctx.arena.get_variable_declaration(decl_node) else {
                    return false;
                };
                (var_data.initializer.is_some(), var_data.exclamation_token)
            } else if decl_node.kind == syntax_kind_ext::BINDING_ELEMENT {
                let Some(var_data) = self.ctx.arena.get_binding_element(decl_node) else {
                    return false;
                };
                // A binding element without its own initializer is still definitely
                // assigned if the enclosing VariableDeclaration has an initializer.
                // In `const [a, b = a] = [1]`, `a` has no pattern-level default but
                // the destructuring `= [1]` assigns all elements left-to-right.
                // Walk up: BindingElement → BindingPattern → ... → VariableDeclaration.
                let init = var_data.initializer.is_some() || {
                    let mut has_parent_init = false;
                    let mut cur = decl_id_to_check;
                    for _ in 0..10 {
                        let Some(info) = self.ctx.arena.node_info(cur) else {
                            break;
                        };
                        let parent = info.parent;
                        let Some(pnode) = self.ctx.arena.get(parent) else {
                            break;
                        };
                        if pnode.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                            if let Some(vd) = self.ctx.arena.get_variable_declaration(pnode) {
                                has_parent_init = vd.initializer.is_some();
                            }
                            break;
                        }
                        cur = parent;
                    }
                    has_parent_init
                };
                (init, false)
            } else {
                return false;
            };

        // Skip TS2454 when the variable is used from a different function scope
        // than where it is declared. The nested function could be called later,
        // so tsz conservatively suppresses TS2454 for captured locals.
        //
        // Source-file globals are more nuanced:
        // - For scripts: suppress TS2454 for reads in deferred (non-IIFE) nested
        //   functions — external scripts could assign the global variable.
        // - For external modules: module-scope variables can only be assigned
        //   within the module. If the variable has ANY assignment anywhere in the
        //   file, suppress (the deferred function might be called after it). If
        //   it has NO assignments at all, emit TS2454 — no code can ever assign it.
        // - In both cases, still report when every crossed function boundary
        //   is an IIFE that executes immediately.
        let decl_scope = self.find_enclosing_function_or_source_file(decl_id_to_check);
        let usage_scope = self.find_enclosing_function_or_source_file(idx);
        if decl_scope != usage_scope && decl_scope.is_some() {
            let decl_is_source_file = self
                .ctx
                .arena
                .get(decl_scope)
                .is_some_and(|node| node.kind == syntax_kind_ext::SOURCE_FILE);
            if !decl_is_source_file {
                return false;
            }

            if self.is_usage_in_deferred_function_relative_to_scope(idx, decl_scope) {
                // In external modules, only suppress if the variable has some
                // assignment reachable in the file. If it is never assigned,
                // the deferred function can never see a valid value.
                //
                // Exception: `var` (function-scoped) declarations are always
                // hoisted and initialized to `undefined` at runtime, so tsc
                // suppresses TS2454 for them in deferred functions regardless
                // of whether an assignment exists in the file.
                let is_function_scoped_var =
                    symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0;
                if self.ctx.binder.is_external_module()
                    && !has_initializer
                    && !is_function_scoped_var
                    && !self.symbol_has_any_assignment_in_file(sym_id)
                {
                    // Fall through — continue to the flow analysis check below
                } else {
                    return false;
                }
            }
        }

        // `var` declarations without initializers AND without type annotations are
        // always `undefined` due to hoisting. tsc does not emit TS2454 for bare
        // `var x;` — only `let`/`const` or `var x: T;` (with type annotation) need
        // definite assignment checking. When a type annotation is present, tsc still
        // requires definite assignment even for `var`.
        // Exception: for-in/for-of loop variables (`for (var v of ...)`) have no
        // initializer in the AST but ARE assigned by the loop — don't suppress for those.
        if !has_initializer {
            let is_function_scoped =
                symbol.flags & tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE != 0;
            if is_function_scoped {
                // Check if this var is a for-in/for-of loop variable
                let is_for_in_of_var = self
                    .ctx
                    .arena
                    .node_info(decl_id_to_check)
                    .and_then(|info| {
                        let list_node = self.ctx.arena.get(info.parent)?;
                        if list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                            return None;
                        }
                        let list_info = self.ctx.arena.node_info(info.parent)?;
                        let for_node = self.ctx.arena.get(list_info.parent)?;
                        Some(
                            for_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                                || for_node.kind == syntax_kind_ext::FOR_OF_STATEMENT,
                        )
                    })
                    .unwrap_or(false);

                if !is_for_in_of_var {
                    let has_type_annotation =
                        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                            let has_ts_annotation = self
                                .ctx
                                .arena
                                .get_variable_declaration(decl_node)
                                .is_some_and(|vd| vd.type_annotation.is_some());
                            // In JS files with checkJs/@ts-check, a JSDoc @type tag
                            // on a `var` declaration acts as a type annotation. tsc
                            // emits TS2454 for `/** @type {X} */ var v;` the same way
                            // it does for `var v: X;`.
                            has_ts_annotation
                                || self
                                    .jsdoc_type_annotation_for_node(decl_id_to_check)
                                    .is_some()
                        } else {
                            false
                        };
                    if !has_type_annotation {
                        return false;
                    }
                }
            }
        }

        // If there's an initializer, skip definite assignment check — unless the variable
        // is `var` (function-scoped) and the usage is before the declaration in source
        // order.  `var` hoists the binding but NOT the initializer, so at the usage
        // point the variable is `undefined`.  Block-scoped variables (let/const) don't
        // need this: TDZ checks handle pre-declaration use separately.
        if has_initializer {
            let is_function_scoped =
                symbol.flags & tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE != 0;

            if is_function_scoped {
                // For `var` with initializer, skip DAA when usage is at or after the
                // declaration in source order. The initializer runs when control flow
                // reaches it; tsc's flow graph handles the "might not execute" case.
                // Only check DAA when usage precedes the declaration (hoisted binding,
                // initializer not yet run).
                //
                // Exception: if the declaration is inside a try block, the initializer
                // may not execute (a prior statement could throw), so we must still
                // run the flow-based DAA check.
                if let Some(usage_node) = self.ctx.arena.get(idx)
                    && usage_node.pos >= decl_node.end
                    && !self.is_inside_try_block(decl_id_to_check)
                {
                    return false;
                }
            } else {
                // Block-scoped (let/const) with initializer: always skip.
                // TDZ checks handle pre-declaration use separately.
                return false;
            }
        }

        // If there's a definite assignment assertion (!), skip check
        if has_exclamation {
            return false;
        }

        // If the variable is declared in a for-in or for-of loop header,
        // it's assigned by the loop iteration itself - but only when usage is at or after the loop.
        // A usage BEFORE the loop in source order (e.g. `v; for (var v of [0]) {}`) must still
        // be checked for definite assignment.
        if let Some(decl_list_info) = self.ctx.arena.node_info(decl_id_to_check) {
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

        // For namespace-scoped variables, skip TS2454 when the usage is inside
        // a nested namespace (MODULE_DECLARATION) relative to the declaration.
        // Flow analysis can't cross namespace boundaries, and the variable may
        // be assigned in the outer namespace before the inner namespace executes.
        // Same-namespace usage still gets TS2454 (flow analysis works within a scope).
        if self.is_usage_in_nested_namespace_from_decl(decl_id_to_check, idx) {
            return false;
        }

        // 1. Skip definite assignment checks in ambient declarations (declare const/let, declare module)
        if self.is_ambient_declaration(decl_id_to_check) {
            return false;
        }

        // 2. Anchor checks to a function-like or source-file container
        let mut current = decl_id_to_check;
        let mut found_container_scope = false;
        for _ in 0..50 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                break;
            };
            if let Some(node) = self.ctx.arena.get(current) {
                // Check if we're inside a function-like or source-file container scope
                if node.is_function_like()
                    || node.kind == syntax_kind_ext::SOURCE_FILE
                    || node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
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

    fn is_inside_class_property_initializer(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        for _ in 0..MAX_TREE_WALK_ITERATIONS {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    return false;
                }
                k if k == syntax_kind_ext::ARROW_FUNCTION => {}
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_property_decl(node)
                        .is_some_and(|prop| prop.initializer.is_some());
                }
                k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                    || k == syntax_kind_ext::SOURCE_FILE =>
                {
                    return false;
                }
                _ => {}
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }

        false
    }

    /// Check if an identifier is inside a `typeof` type query (type position).
    /// e.g., `let b: typeof a;` — the `a` is in type-query position.
    /// This is a compile-time type query, not a runtime value read.
    pub(crate) fn is_in_type_query_position(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                return false;
            };
            let parent = info.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::TYPE_QUERY {
                return true;
            }
            // Stop walking if we hit a statement or declaration boundary
            if parent_node.is_function_like()
                || parent_node.kind == syntax_kind_ext::SOURCE_FILE
                || parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            {
                return false;
            }
            current = parent;
        }
        false
    }

    /// Check if a computed property name node is inside a class body
    /// (as opposed to an object literal). tsc skips TS2454 for class
    /// computed property names but NOT for object literal ones.
    fn is_class_member_computed_property(&self, computed_idx: NodeIndex) -> bool {
        // Walk: ComputedPropertyName -> member (PropertyDeclaration/MethodDecl/etc.) -> ClassDecl/ClassExpr
        let Some(info) = self.ctx.arena.node_info(computed_idx) else {
            return false;
        };
        let member_idx = info.parent;
        let Some(member_info) = self.ctx.arena.node_info(member_idx) else {
            return false;
        };
        let class_idx = member_info.parent;
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        class_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || class_node.kind == syntax_kind_ext::CLASS_EXPRESSION
    }

    pub(crate) fn is_non_null_assertion_operand(&self, idx: NodeIndex) -> bool {
        let Some(info) = self.ctx.arena.node_info(idx) else {
            return false;
        };
        let parent = info.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::NON_NULL_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_unary_expr_ex(parent_node)
            .is_some_and(|expr| expr.expression == idx)
    }

    /// Check if a declaration is inside the `try` block of a `TryStatement`.
    /// When a `var` with initializer is inside a try block, the initializer
    /// may not execute (a prior statement could throw), so the DAA shortcut
    /// must not be applied.
    fn is_inside_try_block(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        for _ in 0..50 {
            let Some(info) = self.ctx.arena.node_info(current) else {
                return false;
            };
            let parent = info.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::TRY_STATEMENT {
                // `current` is a direct child of the TryStatement.
                // The try block is the first Block child. Check if `current`
                // is a Block (the try block) rather than a CatchClause or
                // the finally block.
                if let Some(current_node) = self.ctx.arena.get(current) {
                    return current_node.kind == syntax_kind_ext::BLOCK;
                }
                return false;
            }
            // Stop at function or source-file boundaries
            if parent_node.is_function_like() || parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return false;
            }
            current = parent;
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
    pub(crate) fn is_for_in_of_initializer(&self, idx: NodeIndex) -> bool {
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
    pub(crate) fn is_destructuring_assignment_target(&self, idx: NodeIndex) -> bool {
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

    /// Check if a variable is definitely assigned at a given point,
    /// optionally using a known symbol to pre-seed the flow analyzer's
    /// reference symbol cache. This is needed when `binder.resolve_identifier`
    /// fails to resolve the identifier (e.g., inside `export default expr`),
    /// but the checker has already resolved the symbol through other means.
    pub(crate) fn is_definitely_assigned_at_with_symbol(
        &self,
        idx: NodeIndex,
        known_sym: Option<SymbolId>,
    ) -> bool {
        // Get the flow node for this identifier usage.
        // Identifier reference nodes (e.g., `a` in `console.log(a)`) typically
        // don't have direct flow nodes recorded — the binder only records flow
        // for statements and declarations. Walk up the AST to find the nearest
        // ancestor with a flow node, mirroring `apply_flow_narrowing`'s fallback.
        //
        // IMPORTANT: Compound read-write operations (++, --, +=, -=, **=, etc.)
        // create flow ASSIGNMENT nodes, but the variable is READ before it is
        // written. If the identifier is the operand of such an operation, we must
        // skip that parent's flow node and use a flow node from earlier in the
        // control flow graph. Otherwise `++NUMBER` would see its own assignment
        // and incorrectly conclude that `NUMBER` is definitely assigned.
        let flow_node = if let Some(flow) = self.ctx.binder.get_node_flow(idx) {
            flow
        } else {
            let mut current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
            let mut found = None;
            while let Some(parent) = current {
                if parent.is_none() {
                    break;
                }
                if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                    // Check if this parent is a compound read-write operation
                    // (++, --, +=, -=, **=, etc.) where the identifier is the
                    // target. If so, skip this flow node — the read happens
                    // BEFORE the assignment, so we need the prior flow state.
                    if self.is_compound_read_write_target(parent, idx) {
                        // Skip this flow node and keep walking up
                        current = self.ctx.arena.get_extended(parent).map(|ext| ext.parent);
                        continue;
                    }
                    found = Some(flow);
                    break;
                }
                current = self.ctx.arena.get_extended(parent).map(|ext| ext.parent);
            }
            match found {
                Some(flow) => flow,
                None => {
                    tracing::debug!("No flow info for {idx:?} or its ancestors");
                    // When no flow node exists for this usage or its ancestors
                    // (e.g., identifiers inside JSX attributes, decorator expressions,
                    // computed property names), check whether the variable's declaration
                    // has an initializer. If it lacks an initializer, the variable is
                    // not definitely assigned — returning true here would incorrectly
                    // suppress TS2454.
                    if self.find_enclosing_computed_property(idx).is_some() {
                        return true;
                    }
                    if let Some(sym_id) = known_sym
                        && !self.declaration_has_initializer(sym_id)
                    {
                        return false;
                    }
                    return true;
                }
            }
        };

        // If the flow node is UNREACHABLE, the code at this point will never
        // execute. Skip the definite assignment check — tsc does not emit
        // TS2454 for unreachable code paths (e.g., the incrementor of a
        // for-loop whose body always breaks).
        if let Some(flow) = self.ctx.binder.flow_nodes.get(flow_node)
            && flow.has_any_flags(tsz_binder::flow_flags::UNREACHABLE)
        {
            return true;
        }

        // Create a flow analyzer and check definite assignment
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
        .with_type_environment(&self.ctx.type_environment)
        .with_destructured_bindings(&self.ctx.destructured_bindings);

        // Pre-seed the reference symbol cache when the checker has already
        // resolved the symbol for this reference. This handles cases where
        // `binder.resolve_identifier` fails (e.g., identifiers inside
        // `export default expr`) but the checker resolved the symbol through
        // its own resolution logic.
        if let Some(sym) = known_sym {
            analyzer
                .reference_symbol_cache
                .borrow_mut()
                .insert(idx.0, Some(sym));

            // Invalidate any stale entries in the shared reference match cache
            // that involve this node. A prior `apply_flow_narrowing` call may
            // have cached `false` for `is_matching_reference(assignment, idx)`
            // because `reference_symbol(idx)` returned None without the pre-seeded
            // symbol. With the correct symbol now available, those cached results
            // are invalid and must be recomputed.
            self.ctx
                .flow_reference_match_cache
                .borrow_mut()
                .retain(|&(a, b), _| a != idx.0 && b != idx.0);
        }

        analyzer.is_definitely_assigned(idx, flow_node)
    }

    /// Check if `parent_idx` is a compound read-write operation (prefix/postfix
    /// `++`/`--`, or compound assignment like `+=`, `-=`, `**=`) and `ident_idx`
    /// is the target operand that is read before being written.
    fn is_compound_read_write_target(&self, parent_idx: NodeIndex, ident_idx: NodeIndex) -> bool {
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        // Prefix/postfix ++/-- (e.g., `++x`, `x--`)
        if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
        {
            return (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
                && unary.operand == ident_idx;
        }

        // Compound assignment operators (+=, -=, *=, /=, %=, **=, <<=, >>=, >>>=, &=, |=, ^=, &&=, ||=, ??=)
        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(parent_node)
        {
            let is_compound_assign = matches!(
                bin.operator_token,
                op if op == SyntaxKind::PlusEqualsToken as u16
                    || op == SyntaxKind::MinusEqualsToken as u16
                    || op == SyntaxKind::AsteriskEqualsToken as u16
                    || op == SyntaxKind::SlashEqualsToken as u16
                    || op == SyntaxKind::PercentEqualsToken as u16
                    || op == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                    || op == SyntaxKind::LessThanLessThanEqualsToken as u16
                    || op == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                    || op == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                    || op == SyntaxKind::AmpersandEqualsToken as u16
                    || op == SyntaxKind::BarEqualsToken as u16
                    || op == SyntaxKind::CaretEqualsToken as u16
                    || op == SyntaxKind::BarBarEqualsToken as u16
                    || op == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                    || op == SyntaxKind::QuestionQuestionEqualsToken as u16
            );
            return is_compound_assign && bin.left == ident_idx;
        }

        false
    }

    /// Check if a symbol's value declaration has an initializer.
    /// Used as a fallback when no flow node is found for a usage.
    fn declaration_has_initializer(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let decl_id = symbol.value_declaration;
        if decl_id.is_none() {
            return false;
        }
        let Some(decl_node) = self.ctx.arena.get(decl_id) else {
            return false;
        };
        // Walk from identifier to variable declaration if needed
        let mut decl_node = decl_node;
        let mut decl_id_check = decl_id;
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(info) = self.ctx.arena.node_info(decl_id)
            && let Some(parent) = self.ctx.arena.get(info.parent)
        {
            decl_node = parent;
            decl_id_check = info.parent;
        }
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(var_data) = self.ctx.arena.get_variable_declaration(decl_node)
        {
            return var_data.initializer.is_some();
        }
        if decl_node.kind == syntax_kind_ext::BINDING_ELEMENT {
            return true; // binding elements always have a source
        }
        // For-in/for-of loop variables
        if let Some(decl_list_info) = self.ctx.arena.node_info(decl_id_check) {
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
                    return true;
                }
            }
        }
        false
    }

    /// Find the enclosing function-like node or source file for a given node.
    pub(crate) fn find_enclosing_function_or_source_file(&self, idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        while current.is_some() {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.is_function_like()
                || node.kind == syntax_kind_ext::SOURCE_FILE
                || node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
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

    /// Check whether a use site crosses a deferred function boundary before
    /// reaching the target scope. IIFEs execute immediately and therefore do
    /// not count as deferred boundaries.
    fn is_usage_in_deferred_function_relative_to_scope(
        &self,
        usage_idx: NodeIndex,
        target_scope: NodeIndex,
    ) -> bool {
        let mut current = usage_idx;
        while current.is_some() && current != target_scope {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.is_function_like() && !self.ctx.arena.is_immediately_invoked(current) {
                return true;
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

    /// Check whether a symbol has any flow ASSIGNMENT node in the current file.
    /// Used for external modules to determine if a module-scope variable is
    /// ever assigned — if not, TS2454 should fire even in deferred functions.
    fn symbol_has_any_assignment_in_file(&self, sym_id: SymbolId) -> bool {
        use tsz_binder::flow_flags;
        let flow_arena = &self.ctx.binder.flow_nodes;
        for i in 0..flow_arena.len() {
            let flow_id = tsz_binder::FlowNodeId(i as u32);
            let Some(flow) = flow_arena.get(flow_id) else {
                continue;
            };
            if (flow.flags & flow_flags::ASSIGNMENT) == 0 || flow.node.is_none() {
                continue;
            }
            if self.flow_assignment_targets_symbol(flow.node, sym_id) {
                return true;
            }
        }
        false
    }

    /// Check if a flow assignment's AST node targets the given symbol.
    fn flow_assignment_targets_symbol(&self, node_idx: NodeIndex, target_sym: SymbolId) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        // Binary expression with assignment operator (e.g., `x = value`)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(node)
        {
            return self.node_resolves_to_symbol(bin.left, target_sym);
        }
        // Variable declaration (e.g., `let x = value` — though these have initializers)
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.ctx.arena.get_variable_declaration(node)
        {
            return self.node_resolves_to_symbol(decl.name, target_sym);
        }
        // Prefix/postfix unary (e.g., `++x`, `x--`)
        if (node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.ctx.arena.get_unary_expr(node)
        {
            return self.node_resolves_to_symbol(unary.operand, target_sym);
        }
        // For-in/for-of statement initializer
        if (node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
            && let Some(for_data) = self.ctx.arena.get_for_in_of(node)
        {
            return self.node_resolves_to_symbol(for_data.initializer, target_sym);
        }
        false
    }

    /// Check if a node resolves to the given symbol via the binder.
    fn node_resolves_to_symbol(&self, idx: NodeIndex, target_sym: SymbolId) -> bool {
        if let Some(sym) = self.resolve_for_of_header_expression_symbol(idx) {
            return sym == target_sym;
        }
        if let Some(sym) = self.ctx.binder.get_node_symbol(idx) {
            return sym == target_sym;
        }
        if let Some(sym) = self.resolve_identifier_symbol_without_tracking(idx) {
            return sym == target_sym;
        }
        false
    }
}
