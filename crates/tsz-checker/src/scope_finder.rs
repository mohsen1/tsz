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
//! This module extends CheckerState with scope-finding methods as part of
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
    /// (FunctionDeclaration, FunctionExpression, ArrowFunction, Method, etc.).
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
        use tsz_parser::parser::syntax_kind_ext::*;
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == FUNCTION_DECLARATION
                    || node.kind == FUNCTION_EXPRESSION
                    || node.kind == METHOD_DECLARATION
                    || node.kind == CONSTRUCTOR
                    || node.kind == GET_ACCESSOR
                    || node.kind == SET_ACCESSOR
                {
                    return Some(current);
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

    /// Check if an `arguments` reference is directly inside an arrow function.
    ///
    /// Walks up the AST from the given node. If the first function-like node
    /// encountered is an ArrowFunction, returns true. If it's a regular function
    /// (FunctionDeclaration, FunctionExpression, Method, Constructor, Accessor),
    /// returns false since those have their own `arguments` binding.
    pub(crate) fn is_arguments_in_arrow_function(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::*;
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

    /// Check if `this` has a contextual owner (class or object literal).
    ///
    /// Walks up the AST to find the nearest non-arrow function. If that function is
    /// a class or object literal member (getter, setter, method, constructor), returns
    /// the parent node index. Returns None if not inside such a context.
    ///
    /// Used to suppress false TS2683 when `this` is contextually typed by a class
    /// or object literal but `enclosing_class` is not set on the checker context.
    pub(crate) fn this_has_contextual_owner(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::*;
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
    /// - If we reach a MODULE_DECLARATION without hitting a function boundary,
    ///   `this` is in the namespace body → return true
    pub(crate) fn is_this_in_namespace_body(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::*;
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
    /// 1. `super(this)` — `this` is an argument to the super() call itself
    /// 2. `constructor(x = this.prop)` — `this` in a parameter default of
    ///    a derived class constructor (evaluated before super() can run)
    pub(crate) fn is_this_before_super_in_derived_constructor(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::*;
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
                    if let Some(call_data) = self.ctx.arena.get_call_expr(node) {
                        if let Some(callee) = self.ctx.arena.get(call_data.expression) {
                            if callee.kind == SyntaxKind::SuperKeyword as u16 {
                                // Verify we're in a derived class constructor
                                return self.is_in_derived_class_constructor(current);
                            }
                        }
                    }
                }

                // Pattern 2: this is in a constructor parameter default
                k if k == PARAMETER => {
                    // Check if this parameter belongs to a constructor
                    if let Some(param_ext) = self.ctx.arena.get_extended(current) {
                        let param_parent = param_ext.parent;
                        if let Some(parent_node) = self.ctx.arena.get(param_parent) {
                            if parent_node.kind == CONSTRUCTOR {
                                return self.is_in_derived_class_constructor(param_parent);
                            }
                        }
                    }
                }

                // Stop at any function boundary — this is scoped to the function
                k if k == FUNCTION_DECLARATION
                    || k == FUNCTION_EXPRESSION
                    || k == ARROW_FUNCTION
                    || k == METHOD_DECLARATION
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR
                    || k == CONSTRUCTOR =>
                {
                    return false;
                }

                _ => continue,
            }
        }
    }

    /// Check if a node is inside a constructor of a derived class.
    fn is_in_derived_class_constructor(&self, from_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::*;
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
                return self.class_node_is_derived(class_idx);
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
    fn class_node_is_derived(&self, class_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return false;
        };
        self.class_has_base(class_data)
    }

    // =========================================================================
    // Static Block Enclosure
    // =========================================================================

    /// Find the enclosing static block for a given node.
    ///
    /// Traverses up the AST to find a CLASS_STATIC_BLOCK_DECLARATION.
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
    // Computed Property Enclosure
    // =========================================================================

    /// Find the enclosing computed property name for a given node.
    ///
    /// Traverses up the AST to find a COMPUTED_PROPERTY_NAME.
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

    /// Find the class declaration containing a computed property name.
    ///
    /// Walks up from a computed property to find the containing class member,
    /// then finds the class declaration.
    ///
    /// Returns Some(NodeIndex) if the parent is a class, None otherwise.
    #[allow(dead_code)]
    pub(crate) fn find_class_for_computed_property(
        &self,
        computed_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = computed_idx;
        while !current.is_none() {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    // =========================================================================
    // Heritage Clause Enclosure
    // =========================================================================

    /// Find the enclosing heritage clause (extends/implements) for a node.
    ///
    /// Returns the NodeIndex of the HERITAGE_CLAUSE if the node is inside one.
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

    /// Check if an identifier is the direct expression of an ExpressionWithTypeArguments
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

    /// Find the class or interface declaration containing a heritage clause.
    ///
    /// Given a heritage clause node, returns the parent CLASS_DECLARATION,
    /// CLASS_EXPRESSION, or INTERFACE_DECLARATION.
    ///
    /// Returns Some(NodeIndex) if the parent is a class/interface, None otherwise.
    #[allow(dead_code)]
    pub(crate) fn find_class_for_heritage_clause(
        &self,
        heritage_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let ext = self.ctx.arena.get_extended(heritage_idx)?;
        let parent = ext.parent;
        if parent.is_none() {
            return None;
        }
        let parent_node = self.ctx.arena.get(parent)?;
        if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
        {
            Some(parent)
        } else {
            None
        }
    }
}
