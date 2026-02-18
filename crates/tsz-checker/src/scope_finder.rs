//! Scope Finding Module
//!
//! This module contains methods for finding enclosing scopes and contexts.
//! It handles:
//! - Finding enclosing functions (regular and non-arrow)
//! - Finding enclosing variable statements and declarations
//! - Finding enclosing source files
//! - Finding enclosing static blocks, computed properties, and heritage clauses
//! - Finding class contexts for various member types
//!
//! This module extends `CheckerState` with scope-finding methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Scope Finding Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Function Enclosure
    // =========================================================================

    /// Find the enclosing function for a given node.
    ///
    /// Traverses up the AST to find the first function-like node
    /// (`FunctionDeclaration`, `FunctionExpression`, `ArrowFunction`, Method, etc.).
    ///
    /// Returns Some(NodeIndex) if inside a function, None if at module/global scope.
    pub(crate) fn find_enclosing_function(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current)
                && node.is_function_like()
            {
                return Some(current);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the enclosing NON-ARROW function for a given node.
    ///
    /// Returns Some(NodeIndex) if inside a non-arrow function (function declaration/expression),
    /// None if at module/global scope or only inside arrow functions.
    ///
    /// This is used for `this` type checking: arrow functions capture `this` from their
    /// enclosing scope, so we need to skip past them to find the actual function that
    /// defines the `this` context.
    pub(crate) fn find_enclosing_non_arrow_function(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::{
            CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current)
                && (node.kind == FUNCTION_DECLARATION
                    || node.kind == FUNCTION_EXPRESSION
                    || node.kind == METHOD_DECLARATION
                    || node.kind == CONSTRUCTOR
                    || node.kind == GET_ACCESSOR
                    || node.kind == SET_ACCESSOR)
            {
                return Some(current);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Check if an `arguments` reference is directly inside an arrow function.
    ///
    /// Walks up the AST from the given node. If the first function-like node
    /// encountered is an `ArrowFunction`, returns true. If it's a regular function
    /// (`FunctionDeclaration`, `FunctionExpression`, Method, Constructor, Accessor),
    /// returns false since those have their own `arguments` binding.
    pub(crate) fn is_arguments_in_arrow_function(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    k if k == ARROW_FUNCTION => return true,
                    k if k == FUNCTION_DECLARATION
                        || k == FUNCTION_EXPRESSION
                        || k == METHOD_DECLARATION
                        || k == CONSTRUCTOR
                        || k == GET_ACCESSOR
                        || k == SET_ACCESSOR =>
                    {
                        return false;
                    }
                    _ => {}
                }
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

    /// Returns true if the given node is inside a regular (non-arrow) function body.
    /// Arrow functions don't have their own `arguments` binding, so this returns false for them.
    /// Returns false if at module/global scope (no enclosing function).
    pub(crate) fn is_in_regular_function_body(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    k if k == ARROW_FUNCTION => return false,
                    k if k == FUNCTION_DECLARATION
                        || k == FUNCTION_EXPRESSION
                        || k == METHOD_DECLARATION
                        || k == CONSTRUCTOR
                        || k == GET_ACCESSOR
                        || k == SET_ACCESSOR =>
                    {
                        return true;
                    }
                    _ => {}
                }
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

    /// Check if an `arguments` reference is inside an async non-arrow function/method.
    ///
    /// Returns true when the nearest enclosing function-like node that introduces
    /// an `arguments` binding is async and non-arrow. Arrow functions are excluded
    /// because they are handled by a dedicated ES5 arrow diagnostic path.
    pub(crate) fn is_arguments_in_async_non_arrow_function(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    k if k == ARROW_FUNCTION => return false,
                    k if k == FUNCTION_DECLARATION || k == FUNCTION_EXPRESSION => {
                        return self
                            .ctx
                            .arena
                            .get_function(node)
                            .is_some_and(|f| f.is_async);
                    }
                    k if k == METHOD_DECLARATION => {
                        return self
                            .ctx
                            .arena
                            .get_method_decl(node)
                            .is_some_and(|m| self.has_async_modifier(&m.modifiers));
                    }
                    k if k == CONSTRUCTOR || k == GET_ACCESSOR || k == SET_ACCESSOR => {
                        return false;
                    }
                    _ => {}
                }
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

    /// Returns true when `func_idx` is the executor callback passed to
    /// `new Promise(...)` (first argument, function/arrow expression).
    pub(crate) fn is_promise_executor_function(&self, func_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(func_idx) else {
            return false;
        };
        if ext.parent.is_none() {
            return false;
        }
        let Some(parent) = self.ctx.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(parent) else {
            return false;
        };
        let Some(args) = &call.arguments else {
            return false;
        };
        if args.nodes.first().copied() != Some(func_idx) {
            return false;
        }
        let Some(callee) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        self.ctx
            .arena
            .get_identifier(callee)
            .is_some_and(|i| i.escaped_text == "Promise")
    }

    /// Returns true when the parameter name belongs to a Promise executor callback.
    pub(crate) fn is_parameter_in_promise_executor(&self, param_name_idx: NodeIndex) -> bool {
        let Some(func_idx) = self.find_enclosing_function(param_name_idx) else {
            return false;
        };
        self.is_promise_executor_function(func_idx)
    }

    /// Returns true when the parameter name belongs to an immediately-invoked
    /// function expression.
    pub(crate) fn is_parameter_in_iife(&self, param_name_idx: NodeIndex) -> bool {
        let Some(func_idx) = self.find_enclosing_function(param_name_idx) else {
            return false;
        };
        self.is_immediately_invoked_function(func_idx)
    }

    // Returns true if `func_idx` is an immediately-invoked function expression.
    // Handles wrapped forms like `(function() {})()` and `((x) => x)(1)`.
    pub(crate) fn is_immediately_invoked_function(&self, func_idx: NodeIndex) -> bool {
        let mut current = func_idx;
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                current = parent;
                continue;
            }
            if (parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::NEW_EXPRESSION)
                && let Some(call) = self.ctx.arena.get_call_expr(parent_node)
                && call.expression == current
            {
                return true;
            }
            return false;
        }
    }

    /// Check if `this` has a contextual owner (class or object literal).
    ///
    /// Walks up the AST to find the nearest non-arrow function. If that function is
    /// a class or object literal member (getter, setter, method, constructor), returns
    /// the parent node index. Returns None if not inside such a context.
    ///
    /// Used to suppress false TS2683 when `this` is contextually typed by a class
    /// or object literal but `enclosing_class` is not set on the checker context.
    pub(crate) fn this_has_contextual_owner(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, CLASS_EXPRESSION, CONSTRUCTOR, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, OBJECT_LITERAL_EXPRESSION, PROPERTY_ASSIGNMENT, SET_ACCESSOR,
        };
        let enclosing_fn = self.find_enclosing_non_arrow_function(idx)?;
        let fn_node = self.ctx.arena.get(enclosing_fn)?;

        // Direct class/object literal members: getter, setter, method, constructor
        if fn_node.kind == GET_ACCESSOR
            || fn_node.kind == SET_ACCESSOR
            || fn_node.kind == METHOD_DECLARATION
            || fn_node.kind == CONSTRUCTOR
        {
            let parent = self.ctx.arena.get_extended(enclosing_fn)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == CLASS_DECLARATION
                || parent_node.kind == CLASS_EXPRESSION
                || parent_node.kind == OBJECT_LITERAL_EXPRESSION
            {
                return Some(parent);
            }
        }

        // Function expression as value of an object literal property:
        //   { foo: function() { this; } }
        // Chain: FUNCTION_EXPRESSION → PROPERTY_ASSIGNMENT → OBJECT_LITERAL_EXPRESSION
        if fn_node.kind == FUNCTION_EXPRESSION {
            let parent = self.ctx.arena.get_extended(enclosing_fn)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == PROPERTY_ASSIGNMENT {
                let grandparent = self.ctx.arena.get_extended(parent)?.parent;
                let gp_node = self.ctx.arena.get(grandparent)?;
                if gp_node.kind == OBJECT_LITERAL_EXPRESSION {
                    return Some(grandparent);
                }
            }
        }

        None
    }

    // =========================================================================
    // Namespace Context Detection
    // =========================================================================

    /// Check if a `this` expression is in a module/namespace body context
    /// where it cannot be referenced (TS2331).
    ///
    /// Walks up the AST from the `this` node:
    /// - Arrow functions are transparent (they inherit `this` from outer scope)
    /// - Regular functions/methods/constructors create their own `this` scope,
    ///   so `this` inside them is valid (stops the search)
    /// - For methods/constructors, only the body creates a `this` scope —
    ///   decorator expressions and computed property names execute in the outer scope
    /// - If we reach a `MODULE_DECLARATION` without hitting a function boundary,
    ///   `this` is in the namespace body → return true
    pub(crate) fn is_this_in_namespace_body(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, DECORATOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            GET_ACCESSOR, METHOD_DECLARATION, MODULE_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut in_decorator = false;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            // Track decorator context — decorators execute in the outer scope,
            // not inside the method they decorate
            if node.kind == DECORATOR {
                in_decorator = true;
            }

            match node.kind {
                // Arrow functions don't create their own `this` scope
                k if k == ARROW_FUNCTION => continue,

                // Regular functions always create their own `this` scope
                k if k == FUNCTION_DECLARATION || k == FUNCTION_EXPRESSION => return false,

                // Methods/constructors create `this` scope for their body,
                // but NOT for decorators applied to them
                k if k == METHOD_DECLARATION
                    || k == CONSTRUCTOR
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    if in_decorator {
                        // `this` is in a decorator on this method — not inside
                        // the method body. Continue searching upward.
                        in_decorator = false;
                        continue;
                    }
                    // `this` is inside the method body → has its own scope
                    return false;
                }

                // Reached a namespace/module declaration → TS2331
                k if k == MODULE_DECLARATION => return true,

                _ => continue,
            }
        }
    }

    // =========================================================================
    // Super/This Ordering Detection
    // =========================================================================

    /// Check if a `this` expression is used before `super()` has been called
    /// in a derived class constructor (TS17009).
    ///
    /// Detects two patterns:
    /// 1. `super(this)` — `this` is an argument to the `super()` call itself
    /// 2. `constructor(x = this.prop)` — `this` in a parameter default of
    ///    a derived class constructor (evaluated before `super()` can run)
    /// 3. `this.prop; super();` — direct constructor-body access before first super call
    pub(crate) fn is_this_before_super_in_derived_constructor(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CALL_EXPRESSION, CONSTRUCTOR, FUNCTION_DECLARATION,
            FUNCTION_EXPRESSION, GET_ACCESSOR, METHOD_DECLARATION, PARAMETER, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                // Pattern 1: this is inside super(...) call arguments
                k if k == CALL_EXPRESSION => {
                    if let Some(call_data) = self.ctx.arena.get_call_expr(node)
                        && let Some(callee) = self.ctx.arena.get(call_data.expression)
                        && callee.kind == SyntaxKind::SuperKeyword as u16
                    {
                        // Verify we're in a derived class constructor
                        return self.is_in_derived_class_constructor(current);
                    }
                }

                // Pattern 2: this is in a constructor parameter default
                k if k == PARAMETER => {
                    // Check if this parameter belongs to a constructor
                    if let Some(param_ext) = self.ctx.arena.get_extended(current) {
                        let param_parent = param_ext.parent;
                        if let Some(parent_node) = self.ctx.arena.get(param_parent)
                            && parent_node.kind == CONSTRUCTOR
                        {
                            return self.is_in_derived_class_constructor(param_parent);
                        }
                    }
                }

                // Stop at any function boundary — this is scoped to the function
                k if k == FUNCTION_DECLARATION
                    || k == FUNCTION_EXPRESSION
                    || k == ARROW_FUNCTION
                    || k == METHOD_DECLARATION
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    return false;
                }

                // Pattern 3: direct constructor body access before first super() statement
                k if k == CONSTRUCTOR => {
                    return self.is_this_before_super_in_constructor(current, idx);
                }

                _ => continue,
            }
        }
    }

    fn is_this_before_super_in_constructor(
        &self,
        ctor_idx: NodeIndex,
        this_idx: NodeIndex,
    ) -> bool {
        let Some(ctor_node) = self.ctx.arena.get(ctor_idx) else {
            return false;
        };
        let Some(ctor) = self.ctx.arena.get_constructor(ctor_node) else {
            return false;
        };

        // Only classes that actually require super() are subject to TS17009.
        let Some(ext) = self.ctx.arena.get_extended(ctor_idx) else {
            return false;
        };
        let class_idx = ext.parent;
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };
        if !self.class_requires_super_call(class_data) {
            return false;
        }

        if ctor.body.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(ctor.body) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };

        let Some(first_super_stmt) = block
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&stmt| self.is_super_call_statement(stmt))
        else {
            // No super() call exists in a derived constructor; any `this` usage
            // in the body is still before the required super() initialization.
            return true;
        };

        let Some(super_stmt_node) = self.ctx.arena.get(first_super_stmt) else {
            return false;
        };
        let Some(this_node) = self.ctx.arena.get(this_idx) else {
            return false;
        };

        this_node.pos < super_stmt_node.pos
    }

    /// Check if a node is inside a constructor of a derived class.
    fn is_in_derived_class_constructor(&self, from_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            METHOD_DECLARATION,
        };
        let mut current = from_idx;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            if node.kind == CONSTRUCTOR {
                // Walk up to find the class
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    return false;
                };
                let class_idx = ext.parent;
                return self.class_node_requires_super_call(class_idx);
            }

            // Stop at other function boundaries
            if node.kind == FUNCTION_DECLARATION
                || node.kind == FUNCTION_EXPRESSION
                || node.kind == ARROW_FUNCTION
                || node.kind == METHOD_DECLARATION
            {
                return false;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
    }

    /// Check if a class node (or its parent class) has an extends clause.
    fn class_node_requires_super_call(&self, class_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return false;
        };
        self.class_requires_super_call(class_data)
    }

    // =========================================================================
    // Static Block Enclosure
    // =========================================================================

    /// Find the enclosing static block for a given node.
    ///
    /// Traverses up the AST to find a `CLASS_STATIC_BLOCK_DECLARATION`.
    /// Stops at function boundaries to avoid considering outer static blocks.
    ///
    /// Returns Some(NodeIndex) if inside a static block, None otherwise.
    pub(crate) fn find_enclosing_static_block(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                    return Some(current);
                }
                // Stop at function boundaries (don't consider outer static blocks)
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    // =========================================================================
    // Class Field / Static Block Arguments Check (TS2815)
    // =========================================================================

    /// Check if `arguments` at `idx` is inside a class property initializer
    /// or static block, without a regular function boundary in between.
    ///
    /// Arrow functions are transparent (they don't create their own `arguments`),
    /// so `() => arguments` in a field initializer is still TS2815.
    /// Regular functions (function expressions, methods, constructors, accessors)
    /// create their own `arguments` binding, so the check stops there.
    pub(crate) fn is_arguments_in_class_initializer_or_static_block(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    // Regular function boundaries create their own `arguments` — stop
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::CONSTRUCTOR
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        return false;
                    }
                    // Arrow functions are transparent — continue walking
                    k if k == syntax_kind_ext::ARROW_FUNCTION => {}
                    // Class field initializer — TS2815
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        return true;
                    }
                    // Static block — TS2815
                    k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                        return true;
                    }
                    // Source file — stop
                    k if k == syntax_kind_ext::SOURCE_FILE => {
                        return false;
                    }
                    _ => {}
                }
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

    // =========================================================================
    // Computed Property Enclosure
    // =========================================================================

    /// Find the enclosing computed property name for a given node.
    ///
    /// Traverses up the AST to find a `COMPUTED_PROPERTY_NAME`.
    /// Stops at function boundaries (computed properties inside functions are evaluated at call time).
    ///
    /// Returns Some(NodeIndex) if inside a computed property name, None otherwise.
    pub(crate) fn find_enclosing_computed_property(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    return Some(current);
                }
                // Stop at function boundaries
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Check if `this` is inside a class member's computed property name (TS2465).
    ///
    /// Walks up the parent chain without crossing function boundaries (including
    /// arrow functions). When a `ComputedPropertyName` is found:
    /// - If its owner's parent is a class (`ClassDeclaration`/`ClassExpression`) → return true
    /// - Otherwise (object literal computed property) → keep walking
    ///
    /// This correctly handles nested cases like `class C { [{ [this.x]: 1 }[0]]() {} }`
    /// where `this` is in an object-literal computed property that is itself inside a
    /// class member's computed property.
    pub(crate) fn is_this_in_class_member_computed_property_name(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CLASS_DECLARATION, CLASS_EXPRESSION, COMPUTED_PROPERTY_NAME,
            CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            // Stop at all function boundaries (arrow functions ARE boundaries for `this`)
            if parent_node.kind == FUNCTION_DECLARATION
                || parent_node.kind == FUNCTION_EXPRESSION
                || parent_node.kind == ARROW_FUNCTION
                || parent_node.kind == METHOD_DECLARATION
                || parent_node.kind == CONSTRUCTOR
                || parent_node.kind == GET_ACCESSOR
                || parent_node.kind == SET_ACCESSOR
            {
                return false;
            }
            if parent_node.kind == COMPUTED_PROPERTY_NAME {
                // Check if this computed property's owner's parent is a class
                if let Some(cpn_ext) = self.ctx.arena.get_extended(parent_idx) {
                    let owner_idx = cpn_ext.parent; // MethodDeclaration, PropertyDeclaration, etc.
                    if let Some(owner_ext) = self.ctx.arena.get_extended(owner_idx)
                        && let Some(class_node) = self.ctx.arena.get(owner_ext.parent)
                        && (class_node.kind == CLASS_DECLARATION
                            || class_node.kind == CLASS_EXPRESSION)
                    {
                        return true;
                    }
                }
                // Not a class member computed property; keep walking to find an outer one
            }
            current = parent_idx;
        }
    }

    /// Check if `super` is inside a computed property name in an illegal context (TS2466).
    ///
    /// Mirrors TSC's `getSuperContainer(node, stopOnFunctions=true)` skip semantics:
    ///
    /// When `getSuperContainer` encounters a `ComputedPropertyName`, it performs a
    /// double-advance (skips to the CPN's parent, then advances again), meaning the
    /// direct owner of the computed property name does NOT become the super container.
    /// We simulate this by skipping to the CPN's parent when we encounter one and
    /// continuing the walk from there.
    ///
    /// Legal super containers (reached without skipping through a CPN): methods,
    /// constructors, accessors, static blocks. When found, `super` has a valid context
    /// and we return `false` (not a 2466 error).
    ///
    /// Arrow function handling depends on whether `super` is a call:
    /// - `super()` call: arrow functions ARE boundaries (become the container).
    ///   If the arrow function is the container and we found a CPN → return true.
    /// - `super.x` access: arrow functions are transparent (walked through).
    ///
    /// Correctly handles:
    /// - `class C { [super.bar()]() {} }` → true (class member CPN, no legal container)
    /// - `class C { foo() { var obj = { [super.bar()]() {} }; } }` → false
    ///   (obj-lit CPN inside method `foo()` which IS a legal container)
    /// - `class B { bar() { return class { [super.foo()]() {} } } }` → false
    ///   (nested-class CPN; super's actual container is outer `bar()`)
    /// - `class C { [{ [super.bar()]: 1 }[0]]() {} }` → true
    ///   (inner obj-lit CPN nested inside outer class-member CPN; no legal container)
    /// - `ctor() { super(); () => { var obj = { [(super(), "prop")]() {} } } }` → true
    ///   (`super()` call; arrow fn is boundary; CPN found before boundary)
    pub(crate) fn is_super_in_computed_property_name(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CALL_EXPRESSION, CLASS_STATIC_BLOCK_DECLARATION,
            COMPUTED_PROPERTY_NAME, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            GET_ACCESSOR, METHOD_DECLARATION, PROPERTY_DECLARATION, SET_ACCESSOR,
        };

        // Determine whether this `super` is used as a call (`super()`).
        // For super() calls, TSC does not walk through arrow functions when searching
        // for the super container. For super property accesses, arrow functions are
        // transparent (walked through to find the outer container).
        let is_super_call = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n.kind)))
            .is_some_and(|(parent_idx, parent_kind)| {
                if parent_kind != CALL_EXPRESSION {
                    return false;
                }
                // `super` must be the callee of the call expression
                self.ctx
                    .arena
                    .get_call_expr(self.ctx.arena.get(parent_idx).unwrap())
                    .is_some_and(|call| call.expression == idx)
            });

        let mut current = idx;
        let mut found_computed_property = false;

        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                // Walked off the top of the tree.
                return found_computed_property;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return found_computed_property;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return found_computed_property;
            };

            if parent_node.kind == COMPUTED_PROPERTY_NAME {
                // TSC's getSuperContainer skips ComputedPropertyName by advancing to
                // CPN.parent (the member owner), then the loop advances once more to
                // the member owner's parent. We simulate this: mark that we've found
                // a CPN, then jump to CPN.parent so the next iteration processes
                // CPN.parent.parent.
                found_computed_property = true;
                let Some(cpn_ext) = self.ctx.arena.get_extended(parent_idx) else {
                    return found_computed_property;
                };
                let cpn_owner = cpn_ext.parent;
                if cpn_owner.is_none() {
                    return found_computed_property;
                }
                current = cpn_owner;
                continue;
            }

            // Arrow functions:
            // - For super() calls (isCallExpression=true in TSC): ArrowFunction stops
            //   the getSuperContainer walk and becomes the immediate container. Since
            //   ArrowFunction is never a legal super container (isLegalUsageOfSuperExpression
            //   returns false for it), if we've seen a CPN by now we return true.
            // - For super property accesses: arrow functions are transparent; TSC's
            //   post-container while loop continues through them.
            if parent_node.kind == ARROW_FUNCTION {
                if is_super_call {
                    // Arrow function is the container for this super() call.
                    // isLegalUsageOfSuperExpression(ArrowFunction) = false, so if we
                    // found a CPN between super and this arrow fn, emit TS2466.
                    return found_computed_property;
                }
                // Not a call: transparent, keep walking.
                current = parent_idx;
                continue;
            }

            // Regular function boundaries (stopOnFunctions=true): these become the
            // container. They are not legal super-property-access containers (their
            // parent is not class-like), but this is a different error — not TS2466.
            if parent_node.kind == FUNCTION_DECLARATION || parent_node.kind == FUNCTION_EXPRESSION {
                return false;
            }

            // Legal super container kinds. When reached directly (not via a CPN skip),
            // super is inside a valid class member body and TS2466 does not apply.
            if parent_node.kind == METHOD_DECLARATION
                || parent_node.kind == CONSTRUCTOR
                || parent_node.kind == GET_ACCESSOR
                || parent_node.kind == SET_ACCESSOR
                || parent_node.kind == CLASS_STATIC_BLOCK_DECLARATION
                || parent_node.kind == PROPERTY_DECLARATION
            {
                return false;
            }

            current = parent_idx;
        }
    }

    // =========================================================================
    // Heritage Clause Enclosure
    // =========================================================================

    /// Find the enclosing heritage clause (extends/implements) for a node.
    ///
    /// Returns the `NodeIndex` of the `HERITAGE_CLAUSE` if the node is inside one.
    /// Stops at function/class/interface boundaries.
    ///
    /// Returns Some(NodeIndex) if inside a heritage clause, None otherwise.
    pub(crate) fn find_enclosing_heritage_clause(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == HERITAGE_CLAUSE {
                    return Some(current);
                }
                // Stop at function/class/interface boundaries
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    || node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Check if an identifier is the direct expression of an `ExpressionWithTypeArguments`
    /// in a heritage clause (e.g., `extends A` or `implements B`), as opposed to
    /// being nested deeper (e.g., as a function argument in `extends factory(A)`).
    ///
    /// Returns true ONLY when the identifier is the direct type reference.
    pub(crate) fn is_direct_heritage_type_reference(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        // Walk up from the identifier to the heritage clause.
        // If we encounter a CALL_EXPRESSION on the way, the identifier is
        // nested inside a call (e.g., `factory(A)`) — NOT a direct reference.
        let mut current = idx;
        for _ in 0..20 {
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) if !ext.parent.is_none() => ext,
                _ => return false,
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == HERITAGE_CLAUSE {
                // Reached heritage clause without encountering a call expression.
                // This identifier IS the direct type reference.
                return true;
            }

            // If we pass through a call expression, the identifier is nested
            // (e.g., an argument to `factory(A)`).
            if parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::NEW_EXPRESSION
            {
                return false;
            }

            // Stop at function/class/interface boundaries
            if parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                || parent_node.kind == syntax_kind_ext::SOURCE_FILE
            {
                return false;
            }

            current = parent_idx;
        }
        false
    }
}
