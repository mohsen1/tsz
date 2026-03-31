//! Enclosing scope and context traversal (functions, classes, static blocks).

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Scope Finding Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Check whether a node appears inside a decorator expression.
    ///
    /// Decorators execute in the surrounding scope, not inside the class/member they
    /// decorate, so bare identifier lookup from this region must not see class members.
    pub(crate) fn is_in_decorator_expression(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::DECORATOR;

        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
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
            if node.kind == DECORATOR {
                return true;
            }
        }
        false
    }

    /// Return the declaration or class/member node that owns the nearest enclosing decorator.
    ///
    /// Decorator expressions execute before their owning declaration/class member is
    /// created, so bare-name lookup from within the decorator must ignore declarations
    /// nested inside this owner.
    pub(crate) fn decorator_owner_declaration(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, CLASS_EXPRESSION, CONSTRUCTOR, DECORATOR, GET_ACCESSOR,
            METHOD_DECLARATION, PARAMETER, PROPERTY_DECLARATION, SET_ACCESSOR,
        };

        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
            let node = self.ctx.arena.get(current)?;
            if node.kind == DECORATOR {
                let mut owner = current;
                let mut owner_iterations = 0;
                while owner.is_some() {
                    owner_iterations += 1;
                    if owner_iterations > MAX_TREE_WALK_ITERATIONS {
                        return None;
                    }
                    let ext = self.ctx.arena.get_extended(owner)?;
                    if ext.parent.is_none() {
                        return None;
                    }
                    owner = ext.parent;
                    let node = self.ctx.arena.get(owner)?;
                    if matches!(
                        node.kind,
                        k if k == CLASS_DECLARATION
                            || k == CLASS_EXPRESSION
                            || k == PROPERTY_DECLARATION
                            || k == METHOD_DECLARATION
                            || k == GET_ACCESSOR
                            || k == SET_ACCESSOR
                            || k == CONSTRUCTOR
                            || k == PARAMETER
                    ) {
                        return Some(owner);
                    }
                }
                return None;
            }
        }
        None
    }

    pub(crate) fn node_is_within_decorator_owner(
        &self,
        idx: NodeIndex,
        owner_idx: NodeIndex,
    ) -> bool {
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            if current == owner_idx {
                return true;
            }
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
        }
        false
    }

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
        while current.is_some() {
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
        while current.is_some() {
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

    /// Check if `this` is inside a nested regular function (`FUNCTION_EXPRESSION` or
    /// `FUNCTION_DECLARATION`) within a class body. In such cases, the regular function
    /// creates its own `this` binding, so the enclosing class's `this` type should
    /// not apply. Arrow functions are transparent to `this`, so they don't count.
    ///
    /// Only returns true when there IS an enclosing class context. We derive that
    /// from either active checker state or the AST ancestry so lazy class-member
    /// paths still distinguish nested regular functions from class-owned `this`.
    /// Without a class, a standalone function with an explicit `this` parameter
    /// should use its pushed `this` type from the stack, not fall through to the
    /// TS2683 path.
    pub(crate) fn is_this_in_nested_function_inside_class(&self, idx: NodeIndex) -> bool {
        // When enclosing_class is set (during check_class_member), use it directly.
        // Otherwise derive class ownership from the AST so lazy initializer/type
        // queries still recognize that a nested regular function creates its own
        // `this` binding inside a class member.
        let has_class_context = if self.ctx.enclosing_class.is_some() {
            true
        } else {
            self.nearest_enclosing_class(idx).is_some()
        };
        if !has_class_context {
            return false;
        }
        use tsz_parser::parser::syntax_kind_ext::{FUNCTION_DECLARATION, FUNCTION_EXPRESSION};
        let enclosing_fn = match self.find_enclosing_non_arrow_function(idx) {
            Some(f) => f,
            None => return false,
        };
        let fn_node = match self.ctx.arena.get(enclosing_fn) {
            Some(n) => n,
            None => return false,
        };
        // If the nearest non-arrow function is a regular function (not a class member),
        // then `this` has a new binding and the class `this` doesn't apply.
        fn_node.kind == FUNCTION_EXPRESSION || fn_node.kind == FUNCTION_DECLARATION
    }

    /// Returns true when the nearest non-arrow function is a regular function
    /// that creates its own `this` binding, but does not define what that
    /// binding should be.
    ///
    /// This lets callers ignore an outer contextual/class `this` while still
    /// respecting regular functions that *do* own their `this` through an
    /// explicit `this` parameter, contextual `this`, or object-literal receiver
    /// semantics.
    pub(crate) fn is_this_in_nested_function_without_own_this_binding(
        &mut self,
        idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{FUNCTION_DECLARATION, FUNCTION_EXPRESSION};

        let enclosing_fn = match self.find_enclosing_non_arrow_function(idx) {
            Some(f) => f,
            None => return false,
        };
        let Some(fn_node) = self.ctx.arena.get(enclosing_fn) else {
            return false;
        };

        if fn_node.kind != FUNCTION_EXPRESSION && fn_node.kind != FUNCTION_DECLARATION {
            return false;
        }

        if fn_node.kind == FUNCTION_DECLARATION {
            // Function declarations always create a fresh `this` binding and are
            // never contextually typed by an object-literal receiver. Only an
            // explicit `this` parameter, contextual `this` type, or JS receiver
            // inference should suppress TS2683 for the inner declaration.
            if self.enclosing_function_has_explicit_this_parameter(idx)
                || self.enclosing_function_has_contextual_this_type(idx)
            {
                return false;
            }
            return true;
        }

        if self.this_has_contextual_owner(idx).is_some()
            || self.enclosing_function_has_explicit_this_parameter(idx)
            || self.enclosing_function_has_contextual_this_type(idx)
        {
            return false;
        }

        if fn_node.kind == FUNCTION_EXPRESSION {
            let mut current = enclosing_fn;
            for _ in 0..3 {
                let Some(parent) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
                else {
                    break;
                };
                let Some(parent_node) = self.ctx.arena.get(parent) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                    current = parent;
                    continue;
                }
                if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
                    && binary.right == current
                    && self.is_assignment_operator(binary.operator_token)
                {
                    return false;
                }
                break;
            }
        }

        if self.is_js_file()
            && self
                .ctx
                .arena
                .get_extended(enclosing_fn)
                .map(|ext| ext.parent)
                .is_some_and(|parent| {
                    self.ctx.arena.get(parent).is_some_and(|node| {
                        node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
                    })
                })
        {
            // Top-level/variable-declared JS function expressions participate in
            // constructor-style receiver inference elsewhere in the checker.
            return false;
        }

        // JS files do not get a blanket exemption here. A regular nested
        // function still creates its own `this` binding unless one of the
        // explicit/contextual receiver checks above already claimed ownership.
        true
    }

    /// Find the nearest enclosing class declaration/expression by walking parents.
    pub(crate) fn nearest_enclosing_class(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::{CLASS_DECLARATION, CLASS_EXPRESSION};

        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current)
                && (node.kind == CLASS_DECLARATION || node.kind == CLASS_EXPRESSION)
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

    /// Check if `this` is inside a static class member by walking the AST.
    /// Returns true if the nearest enclosing class member/static-block has a `static` modifier.
    /// Unlike `enclosing_class.in_static_member`, this works even outside the
    /// `check_class_member` flow (e.g., during `get_type_of_node` caching).
    pub(crate) fn is_this_in_static_class_member(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, CLASS_EXPRESSION, CLASS_STATIC_BLOCK_DECLARATION, CONSTRUCTOR,
            FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR, METHOD_DECLARATION,
            PROPERTY_DECLARATION, SET_ACCESSOR,
        };

        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                k if k == PROPERTY_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_property_decl(node)
                        .is_some_and(|prop| self.has_static_modifier(&prop.modifiers));
                }
                k if k == METHOD_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .is_some_and(|method| self.has_static_modifier(&method.modifiers));
                }
                k if k == GET_ACCESSOR || k == SET_ACCESSOR => {
                    return self
                        .ctx
                        .arena
                        .get_accessor(node)
                        .is_some_and(|accessor| self.has_static_modifier(&accessor.modifiers));
                }
                k if k == CLASS_STATIC_BLOCK_DECLARATION => return true,
                k if k == FUNCTION_DECLARATION || k == FUNCTION_EXPRESSION || k == CONSTRUCTOR => {
                    return false;
                }
                k if k == CLASS_DECLARATION || k == CLASS_EXPRESSION => return false,
                _ => {}
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            current = ext.parent;
        }

        false
    }

    /// Check if a node is enclosed by a static class member or static block.
    ///
    /// Unlike `is_this_in_static_class_member`, this intentionally ignores nested
    /// function boundaries because class type parameters remain invalid anywhere
    /// within a static member body, including nested function type annotations and
    /// explicit type-argument lists.
    pub(crate) fn is_in_static_class_member_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, CLASS_EXPRESSION, CLASS_STATIC_BLOCK_DECLARATION, CONSTRUCTOR,
            GET_ACCESSOR, METHOD_DECLARATION, PROPERTY_DECLARATION, SET_ACCESSOR,
        };

        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                k if k == PROPERTY_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_property_decl(node)
                        .is_some_and(|prop| self.has_static_modifier(&prop.modifiers));
                }
                k if k == METHOD_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .is_some_and(|method| self.has_static_modifier(&method.modifiers));
                }
                k if k == GET_ACCESSOR || k == SET_ACCESSOR => {
                    return self
                        .ctx
                        .arena
                        .get_accessor(node)
                        .is_some_and(|accessor| self.has_static_modifier(&accessor.modifiers));
                }
                k if k == CONSTRUCTOR => return false,
                k if k == CLASS_STATIC_BLOCK_DECLARATION => return true,
                k if k == CLASS_DECLARATION || k == CLASS_EXPRESSION => return false,
                _ => {}
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            current = ext.parent;
        }

        false
    }

    /// Check if the enclosing non-arrow function has an explicit `this` parameter.
    ///
    /// TypeScript allows functions to declare `this` as their first parameter
    /// (e.g., `function(this: MyType) { ... }`). When present, `this` is explicitly
    /// typed and TS2683 ("'this' implicitly has type 'any'") must be suppressed.
    pub(crate) fn enclosing_function_has_explicit_this_parameter(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };

        let enclosing_fn = match self.find_enclosing_non_arrow_function(idx) {
            Some(f) => f,
            None => return false,
        };
        let fn_node = match self.ctx.arena.get(enclosing_fn) {
            Some(n) => n,
            None => return false,
        };

        // Get the first parameter based on function kind
        let first_param_idx =
            if fn_node.kind == FUNCTION_DECLARATION || fn_node.kind == FUNCTION_EXPRESSION {
                self.ctx
                    .arena
                    .get_function(fn_node)
                    .and_then(|f| f.parameters.nodes.first().copied())
            } else if fn_node.kind == METHOD_DECLARATION {
                self.ctx
                    .arena
                    .get_method_decl(fn_node)
                    .and_then(|m| m.parameters.nodes.first().copied())
            } else if fn_node.kind == CONSTRUCTOR {
                self.ctx
                    .arena
                    .get_constructor(fn_node)
                    .and_then(|c| c.parameters.nodes.first().copied())
            } else if fn_node.kind == GET_ACCESSOR || fn_node.kind == SET_ACCESSOR {
                self.ctx
                    .arena
                    .get_accessor(fn_node)
                    .and_then(|a| a.parameters.nodes.first().copied())
            } else {
                None
            };

        let Some(param_idx) = first_param_idx else {
            return false;
        };

        // Check if the first parameter is named "this"
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return false;
        };

        // Check if the parameter name is "this" (ThisKeyword or Identifier("this"))
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return true;
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text == "this";
            }
        }
        false
    }

    /// Check if the enclosing function expression has a contextual `this` type
    /// from its parent variable declaration's type annotation.
    ///
    /// When a function expression is assigned to a variable with a type annotation
    /// that includes a `this` parameter (e.g., `const f: (this: Foo) => void = function() {}`),
    /// the `this` type is contextually provided. TS2683 should be suppressed because
    /// the contextual typing pass will properly type `this`.
    pub(crate) fn enclosing_function_has_contextual_this_type(&mut self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{FUNCTION_EXPRESSION, VARIABLE_DECLARATION};

        let enclosing_fn = match self.find_enclosing_non_arrow_function(idx) {
            Some(f) => f,
            None => return false,
        };
        let fn_node = match self.ctx.arena.get(enclosing_fn) {
            Some(n) => n,
            None => return false,
        };

        // Only applies to function expressions (closures), not declarations
        // Exception: In JS files, function declarations can have contextual this types
        // from @constructor or @this JSDoc annotations
        if fn_node.kind != FUNCTION_EXPRESSION && !self.is_js_file() {
            return false;
        }

        // Walk up to find the parent variable declaration
        let parent_idx = match self.ctx.arena.get_extended(enclosing_fn) {
            Some(ext) => ext.parent,
            None => return false,
        };
        let parent_node = match self.ctx.arena.get(parent_idx) {
            Some(n) => n,
            None => return false,
        };

        // Check if parent is a variable declaration with a type annotation
        if parent_node.kind == VARIABLE_DECLARATION {
            let var_decl = match self.ctx.arena.get_variable_declaration(parent_node) {
                Some(d) => d,
                None => return false,
            };
            if var_decl.type_annotation.is_some() {
                // Resolve the type annotation and check if it provides a `this` type
                let declared_type = self.get_type_from_type_node(var_decl.type_annotation);
                let ctx =
                    tsz_solver::ContextualTypeContext::with_expected(self.ctx.types, declared_type);
                if ctx.get_this_type().is_some() {
                    return true;
                }
            }
        }

        if self.is_js_file()
            && let Some(jsdoc_callable_type) =
                self.jsdoc_callable_type_annotation_for_function(enclosing_fn)
        {
            let ctx = tsz_solver::ContextualTypeContext::with_expected(
                self.ctx.types,
                jsdoc_callable_type,
            );
            if ctx.get_this_type().is_some() {
                return true;
            }
        }

        if self.is_js_file()
            && self
                .get_jsdoc_for_function(enclosing_fn)
                .is_some_and(|jsdoc| jsdoc.contains("@this") || jsdoc.contains("@constructor"))
        {
            return true;
        }

        // Check if the function is passed as a callback argument with a contextual this type.
        // When a function is passed as an argument (e.g., `arr.filter(function(x) { this.y })`),
        // the expected parameter type may have a `this` type that should be contextually
        // applied to the callback function.
        if self
            .ctx
            .closures_with_contextual_this_type
            .contains(&enclosing_fn)
        {
            return true;
        }

        false
    }

    /// Check if an `arguments` reference is directly inside an arrow function.
    ///
    /// Check if `this` is inside a top-level arrow function that captures `globalThis`.
    ///
    /// Returns true when `this` is in an arrow function chain that ultimately captures
    /// the global `this`, i.e., there is no enclosing class, object literal, or
    /// non-arrow function providing a local `this` binding.
    ///
    /// Used for TS7041: "The containing arrow function captures the global value of 'this'."
    pub(crate) fn is_this_in_global_capturing_arrow(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CLASS_DECLARATION, CLASS_EXPRESSION, CONSTRUCTOR, FUNCTION_DECLARATION,
            FUNCTION_EXPRESSION, GET_ACCESSOR, METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut found_arrow = false;
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    k if k == ARROW_FUNCTION => {
                        found_arrow = true;
                    }
                    k if k == FUNCTION_DECLARATION
                        || k == FUNCTION_EXPRESSION
                        || k == METHOD_DECLARATION
                        || k == CONSTRUCTOR
                        || k == GET_ACCESSOR
                        || k == SET_ACCESSOR
                        || k == CLASS_DECLARATION
                        || k == CLASS_EXPRESSION =>
                    {
                        // Any of these provide a local `this` binding — not global capture
                        return false;
                    }
                    _ => {}
                }
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        found_arrow
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
        while current.is_some() {
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

    /// Returns true if there is any enclosing regular (non-arrow) function.
    /// Unlike `is_in_regular_function_body`, this walks through arrow functions
    /// since they are transparent for `arguments` capture. Returns false only if
    /// we reach the source file root without finding any regular function.
    pub(crate) fn has_enclosing_regular_function(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    // Arrow functions are transparent — keep walking up
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

    /// Returns true if the given `arguments` reference is inside an arrow function
    /// that captures `arguments` from an outer scope (i.e., the nearest enclosing
    /// arrow does NOT have its own parameter named `arguments`).
    ///
    /// Used for TS2496: `arguments` cannot be referenced in an arrow function in ES5.
    /// When an arrow has `(arguments) => arguments`, the body reference resolves to
    /// the parameter, not the implicit `arguments` object, so TS2496 should not fire.
    pub(crate) fn is_arguments_captured_by_arrow(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR,
            METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return false;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    k if k == ARROW_FUNCTION => {
                        // Check if this arrow has a parameter named "arguments".
                        // If so, the reference resolves to the parameter, not the
                        // implicit arguments object — TS2496 should not fire.
                        if let Some(func) = self.ctx.arena.get_function(node) {
                            for &param_idx in &func.parameters.nodes {
                                if let Some(param_node) = self.ctx.arena.get(param_idx)
                                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                                    && let Some(name_node) = self.ctx.arena.get(param.name)
                                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    && ident.escaped_text == "arguments"
                                {
                                    return false;
                                }
                            }
                        }
                        return true;
                    }
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
        self.ctx.arena.is_immediately_invoked(func_idx)
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
        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == GET_ACCESSOR
                    || k == SET_ACCESSOR
                    || k == METHOD_DECLARATION
                    || k == CONSTRUCTOR =>
                {
                    let parent = self.ctx.arena.get_extended(current)?.parent;
                    let parent_node = self.ctx.arena.get(parent)?;
                    if parent_node.kind == CLASS_DECLARATION
                        || parent_node.kind == CLASS_EXPRESSION
                        || parent_node.kind == OBJECT_LITERAL_EXPRESSION
                    {
                        return Some(parent);
                    }
                    return None;
                }
                k if k == FUNCTION_EXPRESSION => {
                    let parent = self.ctx.arena.get_extended(current)?.parent;
                    let parent_node = self.ctx.arena.get(parent)?;
                    if parent_node.kind == PROPERTY_ASSIGNMENT {
                        let grandparent = self.ctx.arena.get_extended(parent)?.parent;
                        let gp_node = self.ctx.arena.get(grandparent)?;
                        if gp_node.kind == OBJECT_LITERAL_EXPRESSION {
                            return Some(grandparent);
                        }
                    }
                    return None;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => return None,
                _ => {}
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        None
    }

    // =========================================================================
    // Namespace Context Detection
    // =========================================================================

    /// Check if a `this` expression appears inside an enum member initializer where
    /// TypeScript reports TS2332 ("current location").
    ///
    /// Walks up the AST from the `this` node:
    /// - Arrow functions are transparent because they capture the outer `this`
    /// - Regular functions/methods/constructors create a new `this` binding and stop
    ///   the search
    /// - Reaching an enum member before a function boundary means `this` is invalid
    ///   in the enum initializer
    ///
    /// Returns true if `this` at `idx` is inside an enum member initializer
    /// and there is an arrow function between `this` and the enum member.
    /// Used to suppress the TS2683 companion diagnostic in arrow captures.
    pub(crate) fn has_enclosing_arrow_before_enum(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, ENUM_MEMBER, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            GET_ACCESSOR, METHOD_DECLARATION, SET_ACCESSOR,
        };
        let mut current = idx;
        let mut found_arrow = false;
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
                k if k == ARROW_FUNCTION => {
                    found_arrow = true;
                    continue;
                }
                k if k == FUNCTION_DECLARATION
                    || k == FUNCTION_EXPRESSION
                    || k == METHOD_DECLARATION
                    || k == CONSTRUCTOR
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    return false;
                }
                k if k == ENUM_MEMBER => return found_arrow,
                _ => continue,
            }
        }
    }

    pub(crate) fn is_this_in_enum_member_initializer(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CONSTRUCTOR, ENUM_MEMBER, FUNCTION_DECLARATION, FUNCTION_EXPRESSION,
            GET_ACCESSOR, METHOD_DECLARATION, SET_ACCESSOR,
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
                k if k == ARROW_FUNCTION => continue,
                k if k == FUNCTION_DECLARATION
                    || k == FUNCTION_EXPRESSION
                    || k == METHOD_DECLARATION
                    || k == CONSTRUCTOR
                    || k == GET_ACCESSOR
                    || k == SET_ACCESSOR =>
                {
                    return false;
                }
                k if k == ENUM_MEMBER => return true,
                _ => continue,
            }
        }
    }

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

        let mut first_super_pos: Option<u32> = block
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&stmt| self.is_super_call_statement(stmt))
            .and_then(|stmt| self.ctx.arena.get(stmt).map(|n| n.pos));

        if first_super_pos.is_none() {
            let body_idx = ctor.body;
            for i in 0..self.ctx.arena.len() {
                let node_idx = NodeIndex(i as u32);
                if !self.is_descendant_of_node(node_idx, body_idx) && node_idx != body_idx {
                    continue;
                }
                let Some(node) = self.ctx.arena.get(node_idx) else {
                    continue;
                };
                if node.kind != SyntaxKind::SuperKeyword as u16 {
                    continue;
                }
                let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
                    continue;
                };
                let Some(parent) = self.ctx.arena.get(ext.parent) else {
                    continue;
                };
                if parent.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
                    continue;
                }
                let Some(call) = self.ctx.arena.get_call_expr(parent) else {
                    continue;
                };
                if call.expression != node_idx {
                    continue;
                }
                if first_super_pos.is_none_or(|p| node.pos < p) {
                    first_super_pos = Some(node.pos);
                }
            }
        }

        let Some(super_pos) = first_super_pos else {
            // No super() call exists in a derived constructor; any `this` usage
            // in the body is still before the required super() initialization.
            return true;
        };

        let Some(this_node) = self.ctx.arena.get(this_idx) else {
            return false;
        };

        this_node.pos < super_pos
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
        while current.is_some() {
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
                    || node.kind == syntax_kind_ext::GET_ACCESSOR
                    || node.kind == syntax_kind_ext::SET_ACCESSOR
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
        while current.is_some() {
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
        while current.is_some() {
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
                    .get_call_expr(
                        self.ctx
                            .arena
                            .get(parent_idx)
                            .expect("parent_idx obtained from valid extended node"),
                    )
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
        while current.is_some() {
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
                Some(ext) if ext.parent.is_some() => ext,
                _ => return false,
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == HERITAGE_CLAUSE {
                // Suppress TS2693 in ALL heritage clause contexts.
                // The heritage checker emits more specific errors:
                //   - TS2689 for class extending an interface
                //   - TS2507 for non-constructable base expression
                // tsc never emits TS2693 for heritage clause expressions.
                return true;
            }

            // If we pass through a call expression, the identifier might be:
            // (a) the callee (e.g., `color` in `extends color()`) — continue
            //     walking up because `color()` itself might be inside a heritage clause,
            //     and tsc doesn't emit TS2693 for the callee in that context.
            // (b) an argument (e.g., `A` in `extends factory(A)`) — stop, TS2693 applies.
            if parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::NEW_EXPRESSION
            {
                if let Some(call) = self.ctx.arena.get_call_expr(parent_node)
                    && call.expression == current
                {
                    // The identifier is the callee — continue walking up
                    current = parent_idx;
                    continue;
                }
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

    /// Like [`is_direct_heritage_type_reference`] but returns `true` ONLY when
    /// the heritage clause is in a **type-only context** — `interface extends`,
    /// `class implements`, or `declare class extends`.
    ///
    /// For non-ambient `class extends`, this returns `false` because the extends
    /// clause is a **value context** — it needs a constructable runtime value.
    /// When a type-only import is used in `class extends`, TS1361 must fire.
    ///
    /// This is used specifically for the `alias_resolves_to_type_only` path
    /// (TS1361/TS1362 emission).  The broader `is_direct_heritage_type_reference`
    /// is still used for TS2693/TS2708 suppression where ALL heritage clauses
    /// should suppress the generic error.
    pub(crate) fn is_heritage_type_only_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        let mut current = idx;
        for _ in 0..20 {
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) if ext.parent.is_some() => ext,
                _ => return false,
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == HERITAGE_CLAUSE {
                // Found the heritage clause. Check its parent to determine context.
                let hc_ext = match self.ctx.arena.get_extended(parent_idx) {
                    Some(ext) if ext.parent.is_some() => ext,
                    _ => return true, // fallback: suppress
                };
                let hc_parent_idx = hc_ext.parent;
                let Some(hc_parent) = self.ctx.arena.get(hc_parent_idx) else {
                    return true; // fallback: suppress
                };

                // Interface: always type-only context
                if hc_parent.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    return true;
                }

                // Class: check extends vs implements, and declare modifier
                if hc_parent.kind == syntax_kind_ext::CLASS_DECLARATION
                    || hc_parent.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    // `implements` is always a type-only context
                    if let Some(heritage) = self.ctx.arena.get_heritage_clause(parent_node)
                        && heritage.token == SyntaxKind::ImplementsKeyword as u16
                    {
                        return true;
                    }

                    // Ambient class extends (declare class, or class inside
                    // declare namespace/module) → suppress TS1361
                    if self.ctx.arena.is_in_ambient_context(hc_parent_idx) {
                        return true;
                    }

                    // Regular class extends: value context → DON'T suppress
                    return false;
                }

                return true; // other parent: suppress (fallback)
            }

            // Nested inside a call/new: not a direct reference
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

    /// Returns `true` when an identifier is inside a type annotation context
    /// (e.g., as a child of `TypeReference`, `TupleType`, `FunctionType`, etc.).
    ///
    /// In multi-file mode the checker may dispatch type-position identifiers
    /// through `get_type_of_identifier`.  This guard prevents false TS2693 for
    /// type parameters and interfaces used inside type annotations.
    pub(crate) fn is_identifier_in_type_position(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) if ext.parent.is_some() => ext,
                _ => return false,
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent_node.kind {
                // Type nodes: identifier is in a type position
                syntax_kind_ext::TYPE_REFERENCE
                | syntax_kind_ext::TUPLE_TYPE
                | syntax_kind_ext::ARRAY_TYPE
                | syntax_kind_ext::UNION_TYPE
                | syntax_kind_ext::INTERSECTION_TYPE
                | syntax_kind_ext::FUNCTION_TYPE
                | syntax_kind_ext::CONSTRUCTOR_TYPE
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::MAPPED_TYPE
                | syntax_kind_ext::INDEXED_ACCESS_TYPE
                | syntax_kind_ext::CONDITIONAL_TYPE
                | syntax_kind_ext::PARENTHESIZED_TYPE
                | syntax_kind_ext::TYPE_PREDICATE
                | syntax_kind_ext::TYPE_QUERY
                | syntax_kind_ext::TYPE_PARAMETER
                | syntax_kind_ext::PROPERTY_SIGNATURE
                | syntax_kind_ext::METHOD_SIGNATURE
                | syntax_kind_ext::INDEX_SIGNATURE
                | syntax_kind_ext::CALL_SIGNATURE
                | syntax_kind_ext::CONSTRUCT_SIGNATURE => return true,
                // Expression/statement boundaries: stop walking
                syntax_kind_ext::CALL_EXPRESSION
                | syntax_kind_ext::NEW_EXPRESSION
                | syntax_kind_ext::BINARY_EXPRESSION
                | syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::RETURN_STATEMENT
                | syntax_kind_ext::EXPRESSION_STATEMENT
                | syntax_kind_ext::SOURCE_FILE => return false,
                _ => {
                    current = parent_idx;
                }
            }
        }
        false
    }

    /// Returns `true` when the identifier is inside a `typeof` type query
    /// (e.g., `type T = typeof X`).  In type positions, `typeof` is a type
    /// query, not a runtime value usage, so TS1361/TS1362 should be suppressed.
    pub(crate) fn is_in_type_query_context(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..10 {
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
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_QUERY {
                return true;
            }
            // Stop walking if we leave a type context into a statement or expression
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::RETURN_STATEMENT
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
            {
                return false;
            }
            current = parent_idx;
        }
        false
    }

    /// Returns `true` when the identifier is part of an import-equals
    /// declaration's entity name (e.g., `M` in `import r = M.X;`).
    /// In this context, namespace references are not value usages and
    /// should not trigger TS2708.
    pub(crate) fn is_in_import_equals_entity_name(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..10 {
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
            if parent_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                return true;
            }
            // Keep walking through qualified names
            if parent_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                current = parent_idx;
                continue;
            }
            // Any other node kind means we're not in an import-equals entity name
            return false;
        }
        false
    }

    /// Returns `true` when the identifier is being evaluated inside a computed
    /// property name (`[expr]`) that belongs to a type-only or ambient context
    /// (interface member, type literal member, abstract member, `declare`
    /// member, or ambient class).  In these positions the expression is never
    /// emitted as runtime code, so TS1361/TS1362 should be suppressed.
    pub(crate) fn is_in_ambient_computed_property_context(&self) -> bool {
        let Some(cpn_idx) = self.ctx.checking_computed_property_name else {
            return false;
        };

        // Walk from the computed property name node upward to the member
        // declaration, then to its parent (class/interface/type literal).
        let mut current = cpn_idx;
        for _ in 0..8 {
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
                // Interface and type literal members are always type-only
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => return true,
                k if k == syntax_kind_ext::TYPE_LITERAL => return true,

                // Ambient class: `declare class C { [x]: any; }`
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(class) = self.ctx.arena.get_class(parent_node)
                        && self.has_declare_modifier(&class.modifiers)
                    {
                        return true;
                    }
                    return false;
                }

                // Property/method declarations may have abstract or declare modifiers
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(parent_node)
                        && (self.has_abstract_modifier(&prop.modifiers)
                            || self.has_declare_modifier(&prop.modifiers))
                    {
                        return true;
                    }
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(parent_node)
                        && self.has_abstract_modifier(&method.modifiers)
                    {
                        return true;
                    }
                    current = parent_idx;
                }

                _ => {
                    current = parent_idx;
                }
            }
        }
        false
    }
}
