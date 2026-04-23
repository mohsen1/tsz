//! Enclosing scope and context traversal (functions, classes, static blocks).

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug)]
struct SuperInitFlowState {
    super_called: bool,
    reachable: bool,
}

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
            CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, METHOD_DECLARATION,
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
                    || node.is_accessor())
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
            // In JS files, function declarations that have `this.prop = value`
            // assignments in their body are constructor functions. `this` inside
            // them is typed as the constructed instance, not `any`, so TS2683
            // must be suppressed.
            if self.is_js_file() && self.function_body_has_this_property_assignments(enclosing_fn) {
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
                k if k == FUNCTION_DECLARATION || k == FUNCTION_EXPRESSION => {
                    let parent_is_class_member = self
                        .ctx
                        .arena
                        .get_extended(current)
                        .and_then(|ext| self.ctx.arena.get(ext.parent))
                        .is_some_and(|parent| {
                            parent.kind == METHOD_DECLARATION || parent.is_accessor()
                        });
                    if !parent_is_class_member {
                        return false;
                    }
                }
                k if k == CONSTRUCTOR => {
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
            CONSTRUCTOR, FUNCTION_DECLARATION, FUNCTION_EXPRESSION, METHOD_DECLARATION,
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
            } else if fn_node.is_accessor() {
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
        use tsz_parser::parser::syntax_kind_ext::{
            FUNCTION_DECLARATION, FUNCTION_EXPRESSION, VARIABLE_DECLARATION,
        };

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
        // from @constructor or @this JSDoc annotations, or from being treated as constructors
        if fn_node.kind != FUNCTION_EXPRESSION && !self.is_js_file() {
            return false;
        }

        // In JS files, function declarations are treated as potential constructors
        // and will have synthesized instance types for `this`. Suppress TS2683.
        if self.is_js_file() && fn_node.kind == FUNCTION_DECLARATION {
            return true;
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

        if self
            .contextual_this_type_for_call_argument_function(enclosing_fn)
            .is_some()
        {
            return true;
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

        if self.is_js_file()
            && self
                .js_constructor_body_instance_type_for_function(enclosing_fn)
                .is_some()
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

    fn contextual_this_type_for_call_argument_function(
        &mut self,
        fn_idx: NodeIndex,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_EXPRESSION, FUNCTION_EXPRESSION, NEW_EXPRESSION, PARENTHESIZED_EXPRESSION,
        };

        let fn_node = self.ctx.arena.get(fn_idx)?;
        if fn_node.kind != FUNCTION_EXPRESSION {
            return None;
        }

        let mut current = fn_idx;
        loop {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;

            if parent_node.kind == PARENTHESIZED_EXPRESSION {
                current = parent;
                continue;
            }

            if parent_node.kind != CALL_EXPRESSION && parent_node.kind != NEW_EXPRESSION {
                return None;
            }

            let call = self.ctx.arena.get_call_expr(parent_node)?;
            let args = call.arguments.as_ref()?;
            let arg_index = args.nodes.iter().position(|&arg| arg == current)?;

            let callee_type = self.get_type_of_node(call.expression);
            let callee_type = self.evaluate_application_type(callee_type);
            let callee_type = self.resolve_lazy_type(callee_type);
            let callee_type = self.evaluate_contextual_type(callee_type);

            let ctx = tsz_solver::ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                callee_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            let param_type = ctx.get_parameter_type_for_call(arg_index, args.nodes.len())?;
            let param_ctx = tsz_solver::ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                param_type,
                self.ctx.compiler_options.no_implicit_any,
            );

            return param_ctx.get_this_type();
        }
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
}
