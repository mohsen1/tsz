//! Control Flow Type Guard Extraction
//!
//! Split from `control_flow_narrowing.rs` to keep file sizes manageable.
//! Contains mutable/captured variable checks and type guard extraction
//! for flow-based type narrowing (typeof, instanceof, discriminants,
//! type predicates, Array.isArray, array.every).

use tsz_parser::parser::node::CallExprData;
use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{SymbolRef, TypeGuard, TypeId, TypeResolver, TypeofKind};

use super::control_flow::FlowAnalyzer;

impl<'a> FlowAnalyzer<'a> {
    /// Check if a reference node is a mutable variable (let/var) as opposed to const.
    ///
    /// This is critical for closure narrowing - mutable variables cannot preserve
    /// narrowing from outer scope because they may be reassigned through the closure.
    pub(crate) fn is_mutable_variable(&self, reference: NodeIndex) -> bool {
        // Resolve the identifier reference to its symbol
        let Some(symbol_id) = self.binder.resolve_identifier(self.arena, reference) else {
            return false; // No symbol = not a mutable variable
        };

        // Get the symbol's value declaration to check if it's const or let/var
        let Some(symbol) = self.binder.get_symbol(symbol_id) else {
            return false;
        };

        let decl_id = symbol.value_declaration;
        if decl_id == NodeIndex::NONE {
            return false; // No value declaration = not a variable we care about
        }

        // Get the declaration node to check its flags
        let Some(decl_node) = self.arena.get(decl_id) else {
            return false;
        };

        // For variable declarations, the CONST flag is on the VARIABLE_DECLARATION_LIST parent
        // The value_declaration points to VARIABLE_DECLARATION, we need to check its parent's flags
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            // Get the parent (VARIABLE_DECLARATION_LIST) via extended info
            if let Some(ext) = self.arena.get_extended(decl_id)
                && !ext.parent.is_none()
                && let Some(parent_node) = self.arena.get(ext.parent)
            {
                let flags = parent_node.flags as u32;
                let is_const = (flags & node_flags::CONST) != 0;
                return !is_const; // Return true if NOT const (i.e., let or var)
            }
        }

        // For other node types, check the node's own flags
        let flags = decl_node.flags as u32;
        let is_const = (flags & node_flags::CONST) != 0;

        !is_const // Return true if NOT const (i.e., let or var)
    }

    /// Check if a variable is captured from an outer scope (vs declared locally).
    ///
    /// Bug #1.2: Rule #42 should only apply to captured variables, not local variables.
    /// - Variables declared INSIDE the closure should narrow normally
    /// - Variables captured from OUTER scope reset narrowing (for let/var)
    pub(crate) fn is_captured_variable(&self, reference: NodeIndex) -> bool {
        use tsz_binder::ScopeId;
        const MAX_TREE_WALK_ITERATIONS: usize = 1000;

        // Resolve the identifier reference to its symbol
        let Some(symbol_id) = self.binder.resolve_identifier(self.arena, reference) else {
            return false;
        };

        // Get the symbol's value declaration
        let Some(symbol) = self.binder.get_symbol(symbol_id) else {
            return false;
        };

        let decl_id = symbol.value_declaration;
        if decl_id == NodeIndex::NONE {
            return false;
        }

        // Find the enclosing scope of the declaration
        let Some(decl_scope_id) = self.binder.find_enclosing_scope(self.arena, decl_id) else {
            return false;
        };

        // Find the enclosing scope of the usage site on-demand from the reference node.
        // Previously this used `binder.current_scope_id` which is stale after binding
        // completes -- it reflects the binder's final position, not the scope where
        // the reference actually lives.
        let Some(usage_scope_id) = self.binder.find_enclosing_scope(self.arena, reference) else {
            return false;
        };

        // If declared and used in the same scope, not captured
        if decl_scope_id == usage_scope_id {
            return false;
        }

        // Check if declaration scope is an ancestor of usage scope
        let mut scope_id = usage_scope_id;
        let mut iterations = 0;
        while !scope_id.is_none() && iterations < MAX_TREE_WALK_ITERATIONS {
            if scope_id == decl_scope_id {
                return true;
            }

            scope_id = self
                .binder
                .scopes
                .get(scope_id.0 as usize)
                .map_or(ScopeId::NONE, |scope| scope.parent);

            iterations += 1;
        }

        false
    }

    /// Extract a `TypeGuard` from a condition node.
    ///
    /// This method translates AST nodes into AST-agnostic `TypeGuard` enums,
    /// which can then be passed to the Solver's `narrow_type()` method.
    ///
    /// Returns `Some((guard, target, is_optional))` where:
    /// - `guard` is the extracted `TypeGuard`
    /// - `target` is the node being narrowed
    /// - `is_optional` is true for optional chaining calls (?.)
    ///
    /// Returns `None` if the expression is not a recognized guard pattern.
    ///
    /// # Examples
    /// ```ignore
    /// // typeof x === "string" -> Some(TypeGuard::Typeof("string"), x_node, false)
    /// // x === null -> Some(TypeGuard::NullishEquality, x_node, false)
    /// // x.kind === "circle" -> Some(TypeGuard::Discriminant { ... }, x_node, false)
    /// // isString(x) -> Some(TypeGuard::Predicate { ... }, x_node, false)
    /// // obj?.isString(x) -> Some(TypeGuard::Predicate { ... }, x_node, true)
    /// ```
    pub(crate) fn extract_type_guard(
        &self,
        condition: NodeIndex,
    ) -> Option<(TypeGuard, NodeIndex, bool)> {
        let cond_node = self.arena.get(condition)?;

        // Check for call expression (user-defined type guards) FIRST
        if cond_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            return self.extract_call_type_guard(condition);
        }

        // Unwrap assignment expressions: if (flag = (x instanceof Foo)) should extract from RHS
        // TypeScript narrows based on the assigned value, not the assignment itself
        if cond_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(cond_node)
            && bin.operator_token == SyntaxKind::EqualsToken as u16
        {
            // Recursively extract guard from the right-hand side
            return self.extract_type_guard(bin.right);
        }

        let bin = self.arena.get_binary_expr(cond_node)?;

        // Check for instanceof operator: x instanceof MyClass
        if bin.operator_token == SyntaxKind::InstanceOfKeyword as u16 {
            // Target is the left side
            let target = bin.left;
            // Get the constructor type from the right side
            if let Some(instance_type) = self.instance_type_from_constructor(bin.right) {
                return Some((TypeGuard::Instanceof(instance_type), target, false));
            }
            // If we can't get the instance type, still return a guard with OBJECT as fallback
            return Some((TypeGuard::Instanceof(TypeId::OBJECT), target, false));
        }

        // Check for in operator: "prop" in x
        if bin.operator_token == SyntaxKind::InKeyword as u16 {
            // Target is the right side (the object being checked)
            let target = bin.right;
            // Get the property name from the left side
            if let Some((prop_name, _is_number)) = self.in_property_name(bin.left) {
                return Some((TypeGuard::InProperty(prop_name), target, false));
            }
        }

        // Extract the target (left or right side of the comparison)
        let target = self.get_comparison_target(condition)?;

        // Check for typeof comparison: typeof x === "string"
        if let Some(type_name) = self.typeof_comparison_literal(bin.left, bin.right, target)
            && let Some(typeof_kind) = TypeofKind::parse(type_name)
        {
            return Some((TypeGuard::Typeof(typeof_kind), target, false));
        }

        // Check for loose equality with null/undefined: x == null, x != null, x == undefined, x != undefined
        // TypeScript treats these as nullish equality (narrows to null | undefined)
        let is_loose_equality = bin.operator_token == SyntaxKind::EqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16;
        if is_loose_equality
            && let Some(_nullish_type) = self.nullish_comparison(bin.left, bin.right, target)
        {
            // For loose equality with null/undefined, use NullishEquality guard
            // This narrows to null | undefined in true branch, excludes both in false
            return Some((TypeGuard::NullishEquality, target, false));
        }

        // Check for discriminant comparison BEFORE nullish comparison.
        // This is critical for cases like `u.err === undefined` where the target is a
        // property access: discriminant narrowing should narrow the base object `u`,
        // not just the property `u.err`. If discriminant matching fails (e.g., `x === undefined`
        // where `x` is a simple variable), we fall through to nullish comparison.
        if let Some((property_path, literal_type, is_optional, discriminant_base)) =
            self.discriminant_comparison(bin.left, bin.right, target)
        {
            return Some((
                TypeGuard::Discriminant {
                    property_path,
                    value_type: literal_type,
                },
                discriminant_base, // Use the BASE of the property access, not the full access
                is_optional,
            ));
        }

        // Check for strict nullish comparison: x === null, x !== null, x === undefined, x !== undefined
        if let Some(nullish_type) = self.nullish_comparison(bin.left, bin.right, target) {
            return Some((TypeGuard::LiteralEquality(nullish_type), target, false));
        }

        // Check for literal comparison: x === "foo", x === 42
        if let Some(literal_type) = self.literal_comparison(bin.left, bin.right, target) {
            return Some((TypeGuard::LiteralEquality(literal_type), target, false));
        }

        None
    }

    /// Extract a `TypeGuard` from a call expression (user-defined type guard).
    ///
    /// Handles both simple type guards `isString(x)` and `this` guards `obj.isString()`.
    /// Also handles optional chaining `obj?.isString(x)` by returning `is_optional = true`.
    ///
    /// For `asserts x` (no type annotation), returns `TypeGuard::Truthy`.
    ///
    /// # Examples
    /// ```ignore
    /// // isString(x) where isString returns "x is string"
    /// // -> Some(TypeGuard::Predicate { type_id: Some(string), asserts: false }, x_node, false)
    ///
    /// // asserts x is T
    /// // -> Some(TypeGuard::Predicate { type_id: Some(T), asserts: true }, x_node, false)
    ///
    /// // asserts x (no type)
    /// // -> Some(TypeGuard::Truthy, x_node, false)
    ///
    /// // obj?.isString(x)
    /// // -> Some(TypeGuard::Predicate { ... }, x_node, true)
    /// ```
    fn extract_call_type_guard(
        &self,
        condition: NodeIndex,
    ) -> Option<(TypeGuard, NodeIndex, bool)> {
        let call = self.arena.get_call_expr_at(condition)?;

        // Task 10: Check for Array.isArray(x) calls
        if let Some((guard, target)) = self.check_array_is_array(call, condition) {
            let is_optional = self.is_optional_call(condition, call);
            return Some((guard, target, is_optional));
        }

        // Handle ArrayBuffer.isView(x) type guard directly.
        if let Some((guard, target)) = self.check_array_buffer_is_view(call) {
            let is_optional = self.is_optional_call(condition, call);
            return Some((guard, target, is_optional));
        }

        // Check for array.every(predicate) calls
        if let Some((guard, target)) = self.check_array_every_predicate(call, condition) {
            let is_optional = self.is_optional_call(condition, call);
            return Some((guard, target, is_optional));
        }

        // 1. Check for optional chaining on the call
        let is_optional = self.is_optional_call(condition, call);

        // 2. Resolve callee type (skip parens/assertions to handle (isString as any)(x))
        let callee_idx = self.skip_parens_and_assertions(call.expression);
        let callee_type = *self.node_types?.get(&callee_idx.0)?;

        // 3. Get the predicate signature from the callee's type
        let signature = self.predicate_signature_for_type(callee_type)?;

        // 4. Find the target node (the argument or `this` object being narrowed)
        let target_node =
            self.predicate_target_expression(call, &signature.predicate, &signature.params)?;

        // 5. Construct the appropriate guard
        let guard = if let Some(type_id) = signature.predicate.type_id {
            // "x is T" or "asserts x is T"
            TypeGuard::Predicate {
                type_id: Some(type_id),
                asserts: signature.predicate.asserts,
            }
        } else {
            // "asserts x" (no type annotation) - narrows to truthy
            TypeGuard::Truthy
        };

        Some((guard, target_node, is_optional))
    }

    /// Task 10: Check if a call is `Array.isArray(x)`.
    ///
    /// Returns `Some((guard, target))` if this is an Array.isArray call.
    /// The `guard` will be `TypeGuard::Array`, and `target` is the argument expression.
    fn check_array_is_array(
        &self,
        call: &CallExprData,
        _condition: NodeIndex,
    ) -> Option<(TypeGuard, NodeIndex)> {
        // Get the callee (should be a property access: Array.isArray)
        let callee_node = self.arena.get(call.expression)?;
        let access = self.arena.get_access_expr(callee_node)?;

        // Check if the object of the property access is the identifier "Array"
        let obj_text = self
            .arena
            .get(access.expression)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        if obj_text != "Array" {
            return None;
        }

        // Check if the property name is "isArray"
        let prop_text = self
            .arena
            .get(access.name_or_argument)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        if prop_text != "isArray" {
            return None;
        }

        // Get the argument (first argument of Array.isArray call)
        let arg = call.arguments.as_ref()?.nodes.first().copied()?;

        Some((TypeGuard::Array, arg))
    }

    /// Resolve a `SymbolRef` to a proper `Lazy(DefId)` `TypeId` via the `TypeEnvironment`.
    ///
    /// The `TypeEnvironment` maintains the checker's symbol→`DefId` mapping, which
    /// assigns sequential `DefIds` (e.g. 55, 56) different from raw `SymbolIds`.
    /// Using `interner.reference(symbol_ref)` creates `Lazy(DefId(symbol_id))`
    /// which is unresolvable; this method returns the correct `Lazy(DefId)`.
    pub(crate) fn resolve_symbol_to_lazy(&self, symbol_ref: SymbolRef) -> Option<TypeId> {
        let env = self.type_environment.as_ref()?;
        let env_borrowed = env.borrow();
        let def_id = env_borrowed.symbol_to_def_id(symbol_ref)?;
        Some(self.interner.lazy(def_id))
    }

    /// Check if a call is `ArrayBuffer.isView(x)` and return a predicate guard.
    fn check_array_buffer_is_view(&self, call: &CallExprData) -> Option<(TypeGuard, NodeIndex)> {
        let callee_node = self.arena.get(call.expression)?;
        let access = self.arena.get_access_expr(callee_node)?;

        let obj_text = self
            .arena
            .get(access.expression)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;
        if obj_text != "ArrayBuffer" {
            return None;
        }

        let prop_text = self
            .arena
            .get(access.name_or_argument)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;
        if prop_text != "isView" {
            return None;
        }

        let arg = call.arguments.as_ref()?.nodes.first().copied()?;
        let callee_idx = self.skip_parens_and_assertions(call.expression);
        let mut type_id = None;

        if let Some(callee_type) = self
            .node_types
            .and_then(|types| types.get(&callee_idx.0).copied())
        {
            type_id = self
                .predicate_signature_for_type(callee_type)
                .and_then(|signature| signature.predicate.type_id);
        }

        // Only fall back to manual type construction if the type predicate
        // didn't provide a resolved type. The predicate path (above) gives us
        // a properly-resolved TypeId from the checker. For the manual path, we
        // must look up DefIds through the TypeEnvironment (which has the checker's
        // symbol→DefId mappings), not through the interner's `reference()` which
        // creates Lazy(DefId(symbol_id)) — a DefId that doesn't exist in the
        // definition store.
        if type_id.is_none()
            && let Some(sym_id) = self.binder.get_global_type("ArrayBufferView")
        {
            let symbol_ref = SymbolRef(sym_id.0);
            let mut view_type = self
                .resolve_symbol_to_lazy(symbol_ref)
                .unwrap_or_else(|| self.interner.reference(symbol_ref));

            // ArrayBuffer.isView narrows to ArrayBufferView with the default
            // type argument (`ArrayBufferLike`) in TypeScript's lib.
            if let Some(array_buffer_like_sym) = self.binder.get_global_type("ArrayBufferLike") {
                let array_buffer_like_ref = SymbolRef(array_buffer_like_sym.0);
                let array_buffer_like = self
                    .resolve_symbol_to_lazy(array_buffer_like_ref)
                    .unwrap_or_else(|| self.interner.reference(array_buffer_like_ref));

                view_type = self
                    .interner
                    .application(view_type, vec![array_buffer_like]);
            }

            type_id = Some(view_type);
        }

        let type_id = type_id?;

        Some((
            TypeGuard::Predicate {
                type_id: Some(type_id),
                asserts: false,
            },
            arg,
        ))
    }

    /// Check if a call is `array.every(predicate)` where predicate has a type predicate.
    ///
    /// Returns `Some((guard, target))` if this is an array.every call with a type predicate.
    /// The `guard` will narrow the array element type, and `target` is the array expression.
    ///
    /// # Examples
    /// ```typescript
    /// const arr: (number | string)[] = [];
    /// const isString = (x: unknown): x is string => typeof x === 'string';
    /// if (arr.every(isString)) {
    ///   // arr is narrowed to string[]
    /// }
    /// ```
    fn check_array_every_predicate(
        &self,
        call: &CallExprData,
        _condition: NodeIndex,
    ) -> Option<(TypeGuard, NodeIndex)> {
        use tracing::trace;

        trace!("check_array_every_predicate called");

        // Get the callee (should be a property access: array.every)
        let callee_node = self.arena.get(call.expression)?;
        let access = self.arena.get_access_expr(callee_node)?;

        // Check if the property name is "every"
        let prop_text = self
            .arena
            .get(access.name_or_argument)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        trace!(?prop_text, "Property name");

        if prop_text != "every" {
            return None;
        }

        trace!("Found .every() call");

        // Get the first argument (the callback)
        let Some(args) = call.arguments.as_ref() else {
            trace!("No arguments");
            return None;
        };
        let Some(&callback_idx) = args.nodes.first() else {
            trace!("No first argument");
            return None;
        };
        trace!(?callback_idx, "Callback node index");

        // Get the type of the callback
        // During control flow analysis, types might not be cached yet.
        // Try to get from cache first, but if not available, we can't extract the guard
        // (we'd need full CheckerState to compute it, which isn't available in narrowing context).
        let Some(node_types) = self.node_types else {
            trace!("No node_types available");
            return None;
        };
        let Some(&callback_type) = node_types.get(&callback_idx.0) else {
            trace!("Callback type not in node_types - type not computed yet");
            return None;
        };
        trace!(?callback_type, "Callback type");

        // Check if the callback has a type predicate
        let Some(signature) = self.predicate_signature_for_type(callback_type) else {
            trace!("No predicate signature for callback type");
            return None;
        };
        trace!(?signature.predicate, "Found type predicate");

        // Only handle predicates with a type (x is T), not just asserts
        let Some(predicate_type) = signature.predicate.type_id else {
            trace!("No type_id in predicate");
            return None;
        };
        trace!(?predicate_type, "Predicate type ID");

        // The target is the array being called on (access.expression)
        let array_target = access.expression;
        trace!(?array_target, "Array target node");

        // Create an ArrayElementPredicate guard that will narrow the array's element type
        trace!("Creating ArrayElementPredicate guard");
        Some((
            TypeGuard::ArrayElementPredicate {
                element_type: predicate_type,
            },
            array_target,
        ))
    }

    /// Check if a call expression uses optional chaining.
    ///
    /// For `obj?.method(x)`, `func?.()`, or `func?.(x)`, returns `true`.
    /// For `obj.method(x)`, returns `false`.
    fn is_optional_call(&self, call_node_idx: NodeIndex, call: &CallExprData) -> bool {
        // 1. Check if the call node itself has OptionalChain flag (e.g., func?.())
        if let Some(node) = self.arena.get(call_node_idx)
            && (node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0
        {
            return true;
        }

        // 2. Check if the callee is a property access with ?. (e.g., obj?.method())
        if let Some(callee_node) = self.arena.get(call.expression)
            && (callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(callee_node)
            && access.question_dot_token
        {
            return true;
        }

        false
    }

    /// Get the target node being narrowed in a comparison expression.
    ///
    /// For `typeof x === "string"`, returns the node for `x`.
    /// For `x === null`, returns the node for `x`.
    fn get_comparison_target(&self, condition: NodeIndex) -> Option<NodeIndex> {
        let bin = self.arena.get_binary_expr_at(condition)?;

        // For typeof expressions, the target is the operand of typeof
        if let Some(typeof_node) = self.get_typeof_operand(bin.left) {
            return Some(typeof_node);
        }

        // For other comparisons, check if left side is a simple reference
        if self.is_simple_reference(bin.left) {
            return Some(bin.left);
        }

        // Check if right side is a simple reference
        if self.is_simple_reference(bin.right) {
            return Some(bin.right);
        }

        None
    }

    /// Check if a node is a simple reference (identifier or property access).
    fn is_simple_reference(&self, node: NodeIndex) -> bool {
        // Skip parentheses and comma expressions to get the actual reference
        let node = self.skip_parenthesized(node);
        if let Some(node_data) = self.arena.get(node) {
            matches!(
                node_data.kind,
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        } else {
            false
        }
    }

    /// Get the operand of a typeof expression.
    fn get_typeof_operand(&self, node: NodeIndex) -> Option<NodeIndex> {
        let node_data = self.arena.get(node)?;
        if node_data.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return None;
        }

        let unary = self.arena.get_unary_expr(node_data)?;
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return None;
        }

        // Skip parentheses and comma expressions in typeof operand
        // This handles cases like: typeof (a, b).prop
        Some(self.skip_parenthesized(unary.operand))
    }
}
