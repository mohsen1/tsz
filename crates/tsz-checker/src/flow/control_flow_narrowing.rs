//! Control flow narrowing: assignments, predicates, instanceof, in-operator,
//! typeof, discriminants, and literal comparisons.
//!
//! Reference matching, literal parsing, and symbol resolution utilities are in
//! `control_flow_references.rs`.

use rustc_hash::FxHashMap;
use tsz_binder::symbol_flags;
use tsz_common::interner::Atom;
use tsz_parser::parser::node::CallExprData;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{
    NarrowingContext, ParamInfo, TypeGuard, TypeId, TypePredicate, TypePredicateTarget,
    type_queries::{
        PredicateSignatureKind, classify_for_predicate_signature, is_narrowing_literal,
    },
};

use super::control_flow::{FlowAnalyzer, PredicateSignature};

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn assignment_affects_reference(&self, left: NodeIndex, target: NodeIndex) -> bool {
        let left = self.skip_parenthesized(left);
        let target = self.skip_parenthesized(target);
        if self.is_matching_reference(left, target) {
            return true;
        }
        if let Some(base) = self.reference_base(target)
            && self.assignment_affects_reference(left, base)
        {
            return true;
        }

        let Some(node) = self.arena.get(left) else {
            return false;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let Some(access) = self.arena.get_access_expr(node) else {
                return false;
            };
            if access.question_dot_token {
                return false;
            }
            return self.assignment_affects_reference(access.expression, target);
        }

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.assignment_affects_reference(unary.expression, target);
        }

        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.assignment_affects_reference(assertion.expression, target);
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
            && self.is_assignment_operator(bin.operator_token)
        {
            return self.assignment_affects_reference(bin.left, target);
        }

        if (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
            && let Some(lit) = self.arena.get_literal_expr(node)
        {
            for &elem in &lit.elements.nodes {
                if elem.is_none() {
                    continue;
                }
                if self.assignment_affects_reference(elem, target) {
                    return true;
                }
            }
        }

        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_property_assignment(node)
            && self.assignment_affects_reference(prop.initializer, target)
        {
            return true;
        }

        if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_shorthand_property(node)
            && self.assignment_affects_reference(prop.name, target)
        {
            return true;
        }

        if (node.kind == syntax_kind_ext::SPREAD_ELEMENT
            || node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
            && let Some(spread) = self.arena.get_spread(node)
            && self.assignment_affects_reference(spread.expression, target)
        {
            return true;
        }

        if (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            && let Some(pattern) = self.arena.get_binding_pattern(node)
        {
            for &elem in &pattern.elements.nodes {
                if elem.is_none() {
                    continue;
                }
                if self.assignment_affects_reference(elem, target) {
                    return true;
                }
            }
        }

        if node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(binding) = self.arena.get_binding_element(node)
            && self.assignment_affects_reference(binding.name, target)
        {
            return true;
        }

        false
    }

    pub(crate) fn assignment_targets_reference_internal(
        &self,
        left: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let left = self.skip_parenthesized(left);
        let target = self.skip_parenthesized(target);
        if self.is_matching_reference(left, target) {
            return true;
        }

        let Some(node) = self.arena.get(left) else {
            return false;
        };

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.assignment_targets_reference_internal(unary.expression, target);
        }

        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.assignment_targets_reference_internal(assertion.expression, target);
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
            && self.is_assignment_operator(bin.operator_token)
        {
            return self.assignment_targets_reference_internal(bin.left, target);
        }

        if (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
            && let Some(lit) = self.arena.get_literal_expr(node)
        {
            for &elem in &lit.elements.nodes {
                if elem.is_none() {
                    continue;
                }
                if self.assignment_targets_reference_internal(elem, target) {
                    return true;
                }
            }
        }

        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_property_assignment(node)
            && self.assignment_targets_reference_internal(prop.initializer, target)
        {
            return true;
        }

        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_property_assignment(node)
            && self.assignment_targets_reference_internal(prop.initializer, target)
        {
            return true;
        }

        if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_shorthand_property(node)
            && self.assignment_targets_reference_internal(prop.name, target)
        {
            return true;
        }

        if (node.kind == syntax_kind_ext::SPREAD_ELEMENT
            || node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
            && let Some(spread) = self.arena.get_spread(node)
            && self.assignment_targets_reference_internal(spread.expression, target)
        {
            return true;
        }

        if (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            && let Some(pattern) = self.arena.get_binding_pattern(node)
        {
            for &elem in &pattern.elements.nodes {
                if elem.is_none() {
                    continue;
                }
                if self.assignment_targets_reference_internal(elem, target) {
                    return true;
                }
            }
        }

        if node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(binding) = self.arena.get_binding_element(node)
            && self.assignment_targets_reference_internal(binding.name, target)
        {
            return true;
        }

        false
    }

    pub(crate) fn array_mutation_affects_reference(
        &self,
        call: &CallExprData,
        target: NodeIndex,
    ) -> bool {
        let Some(callee_node) = self.arena.get(call.expression) else {
            return false;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        self.assignment_affects_reference(access.expression, target)
    }

    pub(crate) fn narrow_by_call_predicate(
        &self,
        type_id: TypeId,
        call: &CallExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> Option<TypeId> {
        // CRITICAL: Optional chaining behavior for type predicates
        // If call is optional (obj?.method(x)):
        //   - If true branch: method was called, so narrowing applies
        //   - If false branch: method might not have been called, so NO narrowing
        // Check if the callee expression is an optional property access
        if let Some(callee_node) = self.arena.get(call.expression)
            && (callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(callee_node)
            && access.question_dot_token
        {
            // For optional chaining, only narrow the true branch
            if !is_true_branch {
                return None;
            }
        }

        let node_types = self.node_types?;
        let callee_type = *node_types.get(&call.expression.0)?;
        let signature = self.predicate_signature_for_type(callee_type)?;
        let predicate_target =
            self.predicate_target_expression(call, &signature.predicate, &signature.params)?;

        if !self.is_matching_reference(predicate_target, target) {
            return None;
        }

        Some(self.apply_type_predicate_narrowing(type_id, &signature.predicate, is_true_branch))
    }

    pub(crate) fn predicate_signature_for_type(
        &self,
        callee_type: TypeId,
    ) -> Option<PredicateSignature> {
        match classify_for_predicate_signature(self.interner, callee_type) {
            PredicateSignatureKind::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                let predicate = shape.type_predicate.clone()?;
                Some(PredicateSignature {
                    predicate,
                    params: shape.params.clone(),
                })
            }
            PredicateSignatureKind::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                // TODO(Safety): This is a heuristic. We are picking the first signature with a predicate.
                // Correct behavior requires using the specific overload selected by the checker during resolution.
                // If the checker selected a non-predicate overload (e.g. (x: number) => boolean),
                // but we pick a predicate overload (x: string) => x is string, we may narrow incorrectly.
                for sig in &shape.call_signatures {
                    if let Some(predicate) = &sig.type_predicate {
                        return Some(PredicateSignature {
                            predicate: predicate.clone(),
                            params: sig.params.clone(),
                        });
                    }
                }
                None
            }
            PredicateSignatureKind::Union(members) => {
                // CRITICAL FIX: For Union, ALL members must have the same predicate
                // If the type is A | B and only A has a predicate, we can't safely narrow
                let mut common_sig: Option<PredicateSignature> = None;

                for member in members {
                    let sig = self.predicate_signature_for_type(member)?;

                    if let Some(ref common) = common_sig {
                        // Simplified check: predicates must match exactly
                        // (Real TS does subtype compatibility check, but identity is safe for now)
                        if common.predicate != sig.predicate {
                            return None;
                        }
                    } else {
                        common_sig = Some(sig);
                    }
                }
                common_sig
            }
            PredicateSignatureKind::Intersection(members) => {
                // For intersections, search ALL members and return the first predicate found
                // Intersections of functions are rare but possible (e.g., overloaded functions)
                // In an intersection A & B, if A has a predicate, the intersection has that predicate
                for member in members {
                    if let Some(sig) = self.predicate_signature_for_type(member) {
                        return Some(sig);
                    }
                }
                None
            }
            PredicateSignatureKind::None => None,
        }
    }

    pub(crate) fn predicate_target_expression(
        &self,
        call: &CallExprData,
        predicate: &TypePredicate,
        params: &[ParamInfo],
    ) -> Option<NodeIndex> {
        match predicate.target {
            TypePredicateTarget::Identifier(name) => {
                let param_index = params.iter().position(|param| param.name == Some(name))?;
                let args = call.arguments.as_ref()?.nodes.as_slice();

                // Walk through arguments, accounting for spread elements.
                // A spread argument expands to an unknown number of positional args,
                // so once we encounter one we can no longer map param_index to a
                // specific argument expression — bail out.
                for (arg_pos, &arg_idx) in args.iter().enumerate() {
                    if let Some(arg_node) = self.arena.get(arg_idx)
                        && arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    {
                        return None;
                    }
                    if arg_pos == param_index {
                        return Some(arg_idx);
                    }
                }
                None
            }
            TypePredicateTarget::This => {
                // CRITICAL: Skip parens/assertions to find the actual access node
                // Handles cases like (obj.isString)() and (obj.isString as any)()
                let callee_idx = self.skip_parens_and_assertions(call.expression);
                // Check for PropertyAccess or ElementAccess
                if let Some(access) = self.arena.get_access_expr_at(callee_idx) {
                    return Some(access.expression);
                }

                None
            }
        }
    }

    /// Resolve a generic assertion predicate's type from the call's actual argument types.
    ///
    /// For `assertEqual<T>(value: any, type: T): asserts value is T` called as
    /// `assertEqual(animal.type, 'cat' as const)`, the predicate's `type_id` is the
    /// unresolved type parameter T. This method finds which parameter shares that type
    /// and resolves it to the corresponding argument's concrete type (e.g., literal 'cat').
    pub(crate) fn resolve_generic_predicate(
        &self,
        predicate: &TypePredicate,
        params: &[ParamInfo],
        call: &CallExprData,
        callee_type: TypeId,
        node_types: &FxHashMap<u32, TypeId>,
    ) -> TypePredicate {
        let Some(pred_type) = predicate.type_id else {
            return predicate.clone();
        };

        // Check if the callee is a generic function/callable with type params
        let has_type_params = match classify_for_predicate_signature(self.interner, callee_type) {
            PredicateSignatureKind::Function(shape_id) => !self
                .interner
                .function_shape(shape_id)
                .type_params
                .is_empty(),
            PredicateSignatureKind::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty())
            }
            _ => false,
        };

        if !has_type_params {
            return predicate.clone();
        }

        // Find which parameter has the same type as the predicate type
        let args = match call.arguments.as_ref() {
            Some(args) => args.nodes.as_slice(),
            None => return predicate.clone(),
        };

        for (i, param) in params.iter().enumerate() {
            if param.type_id == pred_type {
                // This parameter's declared type matches the predicate type (both are T)
                // Get the corresponding argument's concrete type
                if let Some(&arg_idx) = args.get(i)
                    && let Some(&arg_type) = node_types.get(&arg_idx.0)
                {
                    return TypePredicate {
                        type_id: Some(arg_type),
                        ..predicate.clone()
                    };
                }
            }
        }

        predicate.clone()
    }

    pub(crate) fn apply_type_predicate_narrowing(
        &self,
        type_id: TypeId,
        predicate: &TypePredicate,
        is_true_branch: bool,
    ) -> TypeId {
        if predicate.asserts && !is_true_branch {
            return type_id;
        }

        // Create narrowing context and wire up TypeEnvironment if available
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
        } else {
            NarrowingContext::new(self.interner)
        };

        if let Some(predicate_type) = predicate.type_id {
            // Route through TypeGuard::Predicate for proper intersection semantics.
            // When source and target don't overlap (e.g. successive type guards
            // hasLegs then hasWings), the solver falls back to intersection.
            let guard = TypeGuard::Predicate {
                type_id: Some(predicate_type),
                asserts: predicate.asserts,
            };
            return narrowing.narrow_type(type_id, &guard, is_true_branch);
        }

        // Assertion guards without type predicate (asserts x) narrow to truthy
        // This is the CRITICAL fix: use TypeGuard::Truthy instead of just excluding null/undefined
        if is_true_branch {
            // Delegate to narrow_type with TypeGuard::Truthy for comprehensive narrowing
            return narrowing.narrow_type(type_id, &TypeGuard::Truthy, true);
        }

        // Use Solver's narrow_to_falsy for correct NaN handling
        narrowing.narrow_to_falsy(type_id)
    }

    pub(crate) fn narrow_by_instanceof(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        if !self.is_matching_reference(bin.left, target) {
            return type_id;
        }

        // When the constructor expression is typed as `any`, instanceof narrowing is not
        // well-defined — TypeScript keeps the source type unchanged in this case.
        let constructor_expr_type = self.node_types.and_then(|nt| nt.get(&bin.right.0).copied());
        if constructor_expr_type == Some(TypeId::ANY) {
            return type_id;
        }

        // Extract instance type from constructor expression (AST -> TypeId)
        let instance_type = self
            .instance_type_from_constructor(bin.right)
            .unwrap_or(TypeId::OBJECT);

        // TypeScript rule: "any" is not narrowed when target type is "Function" or "Object"
        if type_id == TypeId::ANY {
            let right_idx = self.skip_parenthesized(bin.right);
            if let Some(constructor_node) = self.arena.get(right_idx)
                && constructor_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(ident) = self.arena.get_identifier(constructor_node)
                && (ident.escaped_text == "Function" || ident.escaped_text == "Object")
            {
                return type_id;
            }
        }

        // Delegate to solver via unified narrow_type API
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
        } else {
            NarrowingContext::new(self.interner)
        };

        // Delegate all type algebra to solver - it handles all fallback cases:
        // 1. Instance type narrowing
        // 2. Intersection fallback for interface vs class
        // 3. Object-like filtering for primitives
        narrowing.narrow_type(
            type_id,
            &TypeGuard::Instanceof(instance_type),
            is_true_branch,
        )
    }

    pub(crate) fn instance_type_from_constructor(&self, expr: NodeIndex) -> Option<TypeId> {
        if let Some(node_types) = self.node_types
            && let Some(&type_id) = node_types.get(&expr.0)
            && let Some(instance_type) =
                tsz_solver::type_queries::instance_type_from_constructor(self.interner, type_id)
        {
            return Some(instance_type);
        }

        let expr = self.skip_parens_and_assertions(expr);
        let sym_id = self.binder.resolve_identifier(self.arena, expr)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        let symbol_ref = tsz_solver::SymbolRef(sym_id.0);
        if (symbol.flags & symbol_flags::CLASS) != 0 {
            return Some(
                self.resolve_symbol_to_lazy(symbol_ref)
                    .unwrap_or_else(|| self.interner.reference(symbol_ref)),
            );
        }

        // Global constructor variables (e.g., `declare var Array: ArrayConstructor`)
        // have both INTERFACE and VARIABLE flags. The interface type IS the instance type
        // since interfaces describe instances, not constructors.
        // This handles `x instanceof Array`, `x instanceof Date`, etc.
        if (symbol.flags & symbol_flags::INTERFACE) != 0
            && (symbol.flags & symbol_flags::VARIABLE) != 0
        {
            return Some(
                self.resolve_symbol_to_lazy(symbol_ref)
                    .unwrap_or_else(|| self.interner.reference(symbol_ref)),
            );
        }

        None
    }

    pub(crate) fn narrow_by_in_operator(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        // AST extraction: check if we're narrowing the right reference
        if !self.is_matching_reference(bin.right, target) {
            return type_id;
        }

        // AST extraction: get property name from left side of `in` operator
        let Some((prop_name, _prop_is_number)) = self.in_property_name(bin.left) else {
            return type_id;
        };

        // Delegate ALL type algebra to solver via unified narrow_type API
        // Solver handles: ANY, UNKNOWN, type parameters, unions, non-union types
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
        } else {
            NarrowingContext::new(self.interner)
        };

        narrowing.narrow_type(type_id, &TypeGuard::InProperty(prop_name), is_true_branch)
    }

    pub(crate) fn in_property_name(&self, idx: NodeIndex) -> Option<(Atom, bool)> {
        let idx = self.skip_parenthesized(idx);

        // Handle private identifiers (e.g., `#field in obj`)
        if let Some(node) = self.arena.get(idx)
            && node.kind == SyntaxKind::PrivateIdentifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
        {
            return Some((self.interner.intern_string(&ident.escaped_text), false));
        }

        self.literal_atom_and_kind_from_node_or_type(idx)
    }

    pub(crate) fn skip_parenthesized(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            // Skip non-null assertions (expr!) — TypeScript treats these as transparent
            // for narrowing purposes, so `x!.prop` should narrow the same as `x.prop`.
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            // Skip comma expressions - they evaluate to their rightmost operand
            // This allows narrowing to work through expressions like (a, b).prop
            // Fast path: check kind first before calling get_binary_expr
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.arena.get_binary_expr(node)
                && bin.operator_token == SyntaxKind::CommaToken as u16
            {
                idx = bin.right;
                continue;
            }
            return idx;
        }
    }

    pub(crate) fn skip_parens_and_assertions(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            idx = self.skip_parenthesized(idx);
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if (node.kind == syntax_kind_ext::TYPE_ASSERTION
                || node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
                && let Some(assertion) = self.arena.get_type_assertion(node)
            {
                idx = assertion.expression;
                continue;
            }
            return idx;
        }
    }

    pub(crate) fn typeof_comparison_literal(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<&str> {
        if self.is_typeof_target(left, target) {
            return self.literal_string_from_node(right);
        }
        if self.is_typeof_target(right, target) {
            return self.literal_string_from_node(left);
        }
        None
    }

    pub(crate) fn is_typeof_target(&self, expr: NodeIndex, target: NodeIndex) -> bool {
        let expr = self.skip_parenthesized(expr);
        let node = match self.arena.get(expr) {
            Some(node) => node,
            None => return false,
        };

        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }

        let Some(unary) = self.arena.get_unary_expr(node) else {
            return false;
        };

        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return false;
        }

        self.is_matching_reference(unary.operand, target)
    }

    pub(crate) fn literal_string_from_node(&self, idx: NodeIndex) -> Option<&str> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.as_str());
        }

        // Handle private identifiers (e.g., #a) for `in` operator narrowing
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.as_str());
        }

        None
    }

    pub(crate) fn literal_type_from_node(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.arena.get_literal(node)?;
                Some(self.interner.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                Some(self.interner.literal_number(value))
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                let normalized = self.normalize_bigint_literal(text)?;
                Some(self.interner.literal_bigint(normalized.as_ref()))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(self.interner.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(self.interner.literal_boolean(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }

                let operand = self.skip_parenthesized(unary.operand);
                let operand_node = self.arena.get(operand)?;
                match operand_node.kind {
                    k if k == SyntaxKind::NumericLiteral as u16 => {
                        let lit = self.arena.get_literal(operand_node)?;
                        let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                        let value = if op == SyntaxKind::MinusToken as u16 {
                            -value
                        } else {
                            value
                        };
                        Some(self.interner.literal_number(value))
                    }
                    k if k == SyntaxKind::BigIntLiteral as u16 => {
                        let lit = self.arena.get_literal(operand_node)?;
                        let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                        let normalized = self.normalize_bigint_literal(text)?;
                        let negative = op == SyntaxKind::MinusToken as u16;
                        Some(
                            self.interner
                                .literal_bigint_with_sign(negative, normalized.as_ref()),
                        )
                    }
                    _ => None,
                }
            }
            _ => {
                // Handle `undefined` in value position (it's an Identifier, not UndefinedKeyword)
                if let Some(ident) = self.arena.get_identifier(node)
                    && ident.escaped_text == "undefined"
                {
                    return Some(TypeId::UNDEFINED);
                }
                // Fallback: look up the already-computed type for this expression.
                // This handles enum member access (e.g., Types.Str), const enum members,
                // and other expressions that evaluate to literal or enum types.
                let node_types = self.node_types?;
                let &type_id = node_types.get(&idx.0)?;
                is_narrowing_literal(self.interner, type_id)
            }
        }
    }

    pub(crate) fn nullish_literal_type(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::NullKeyword as u16 {
            return Some(TypeId::NULL);
        }
        if node.kind == SyntaxKind::UndefinedKeyword as u16 {
            return Some(TypeId::UNDEFINED);
        }
        // In value position, `undefined` is an Identifier, not UndefinedKeyword
        if let Some(ident) = self.arena.get_identifier(node)
            && ident.escaped_text == "undefined"
        {
            return Some(TypeId::UNDEFINED);
        }

        None
    }

    pub(crate) fn nullish_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_matching_reference(left, target) {
            return self.nullish_literal_type(right);
        }
        if self.is_matching_reference(right, target) {
            return self.nullish_literal_type(left);
        }
        None
    }

    pub(crate) fn discriminant_property(
        &self,
        expr: NodeIndex,
        target: NodeIndex,
    ) -> Option<Vec<Atom>> {
        self.discriminant_property_info(expr, target)
            .and_then(|(path, is_optional, base)| {
                if is_optional {
                    return None;
                }
                // Only apply discriminant narrowing if the base of the property
                // access matches the target being narrowed. For example, if narrowing
                // `x` based on `x.kind`, the base `x` must match target `x`.
                // Without this check, narrowing `x.prop` based on `x.kind` would
                // incorrectly try to find `kind` on the type of `x.prop`.
                self.is_matching_reference(base, target).then_some(path)
            })
    }

    /// For a const-declared identifier that is a destructuring alias,
    /// return the (`base_initializer`, `property_name`).
    ///
    /// Example: `const { type: alias } = obj` → `(obj, "type")`
    ///
    /// Returns `None` for non-identifiers, non-const bindings, or nested patterns.
    fn binding_element_property_alias(&self, node: NodeIndex) -> Option<(NodeIndex, Atom)> {
        let node_data = self.arena.get(node)?;
        if node_data.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.binder.resolve_identifier(self.arena, node)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        // Must be a block-scoped (const/let) variable
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return None;
        }
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        // `value_declaration` for destructuring bindings may point to the identifier node
        // (the name/alias) rather than the BINDING_ELEMENT itself, because the binder calls
        // `declare_symbol(name_ident, ...)`. In that case, walk up to the parent to find
        // the actual BINDING_ELEMENT.
        let decl_idx = if decl_node.kind == SyntaxKind::Identifier as u16 {
            let ext = self.arena.get_extended(decl_idx)?;
            ext.parent
        } else {
            decl_idx
        };
        let decl_node = self.arena.get(decl_idx)?;
        // Must be a binding element (from object destructuring)
        if decl_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return None;
        }
        let be = self.arena.get_binding_element(decl_node)?;
        // Must not be a rest element (`...rest`)
        if be.dot_dot_dot_token {
            return None;
        }
        // Must not have a default initializer (const { type: alias = "default" } = ...)
        if be.initializer.is_some() {
            return None;
        }
        // Get the property name being destructured
        // `{ type: alias }` → property_name node is "type"
        // `{ type }` shorthand → name node IS the property name
        let prop_name_idx = if be.property_name.is_some() {
            be.property_name
        } else {
            be.name
        };
        let prop_name_node = self.arena.get(prop_name_idx)?;
        let prop_ident = self.arena.get_identifier(prop_name_node)?;
        let prop_name = self.interner.intern_string(&prop_ident.escaped_text);

        // Walk up: BindingElement → ObjectBindingPattern → VariableDeclaration
        let be_ext = self.arena.get_extended(decl_idx)?;
        let binding_pattern_idx = be_ext.parent;
        if binding_pattern_idx.is_none() {
            return None;
        }
        let binding_pattern_node = self.arena.get(binding_pattern_idx)?;
        if binding_pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }
        let bp_ext = self.arena.get_extended(binding_pattern_idx)?;
        let var_decl_idx = bp_ext.parent;
        if var_decl_idx.is_none() {
            return None;
        }
        let var_decl_node = self.arena.get(var_decl_idx)?;
        if var_decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        if !self.is_const_variable_declaration(var_decl_idx) {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(var_decl_node)?;
        if var_decl.initializer.is_none() {
            return None;
        }
        let base = self.skip_parenthesized(var_decl.initializer);
        Some((base, prop_name))
    }

    pub(crate) fn discriminant_property_info(
        &self,
        expr: NodeIndex,
        _target: NodeIndex,
    ) -> Option<(Vec<Atom>, bool, NodeIndex)> {
        let expr = self.skip_parenthesized(expr);
        let _node = self.arena.get(expr)?;

        // Collect the property path by walking up the access chain
        // For action.payload.kind, we want ["payload", "kind"]
        let mut path: Vec<Atom> = Vec::new();
        let mut is_optional = false;
        let mut current = expr;

        loop {
            let current_node = self.arena.get(current)?;
            let access = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || current_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                self.arena.get_access_expr(current_node)?
            } else {
                // Not a property/element access - we've reached the base
                break;
            };

            // Track if any segment uses optional chaining
            if access.question_dot_token {
                is_optional = true;
            }

            // Get the property name for this segment
            let prop_name = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let ident = self.arena.get_identifier_at(access.name_or_argument)?;
                self.interner.intern_string(&ident.escaped_text)
            } else {
                // Element access
                self.literal_atom_from_node_or_type(access.name_or_argument)?
            };

            // Add to path (will be reversed later)
            path.push(prop_name);

            // Move to the next level up
            let access_target = access.expression;
            let access_target = self.skip_parenthesized(access_target);
            let access_target_node = self.arena.get(access_target)?;

            // Unwrap assignment and comma expressions to get the actual target
            let effective_target = if access_target_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            {
                let binary = self.arena.get_binary_expr(access_target_node)?;
                if binary.operator_token == SyntaxKind::EqualsToken as u16 {
                    // (x = y).prop -> unwrap to x.prop
                    binary.left
                } else if binary.operator_token == SyntaxKind::CommaToken as u16 {
                    // (a, b).prop -> unwrap to b.prop
                    binary.right
                } else {
                    access_target
                }
            } else {
                access_target
            };

            current = effective_target;
        }

        // Reverse the path to get correct order (["payload", "kind"] not ["kind", "payload"])
        path.reverse();

        if path.is_empty() {
            return None;
        }

        // current is now the base (e.g., "action" in action.payload.kind)
        Some((path, is_optional, current))
    }

    pub(crate) fn discriminant_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, TypeId, bool, NodeIndex)> {
        // Use relative_discriminant_path to find the property path from target to left.
        // This correctly handles both:
        //   - Direct: `t.kind === "a"` narrowing `t` → path=["kind"], base=t
        //   - Nested: `this.test.type === "a"` narrowing `this.test` → path=["type"], base=this.test
        // (discriminant_property_info returns the full path from the root, which is wrong when
        //  target is not the root — e.g., returns path=["test","type"] base=this for `this.test.type`
        //  when we need path=["type"] base=this.test relative to target=this.test)
        if let Some(literal) = self.literal_type_from_node(right)
            && let Some((rel_path, is_optional)) = self.relative_discriminant_path(left, target)
            && !rel_path.is_empty()
        {
            return Some((rel_path, literal, is_optional, target));
        }

        if let Some(literal) = self.literal_type_from_node(left)
            && let Some((rel_path, is_optional)) = self.relative_discriminant_path(right, target)
            && !rel_path.is_empty()
        {
            return Some((rel_path, literal, is_optional, target));
        }

        // Try aliased discriminant: const alias = target.prop (or target.a.b)
        // where alias is a const identifier initialized from a property access of target.
        // e.g., `const testType = this.test.type` and target = `this.test`
        //   → path = ["type"], base = this.test
        // Also handles destructuring: `const { type: alias } = target`
        //   → path = ["type"], base = target
        if let Some(result) = self.aliased_discriminant(left, right, target) {
            return Some(result);
        }
        if let Some(result) = self.aliased_discriminant(right, left, target) {
            return Some(result);
        }

        None
    }

    /// Try to extract a discriminant guard for an aliased condition.
    ///
    /// Handles:
    /// - `const alias = target.prop` → `alias === literal` narrows `target` by `prop`
    /// - `const { prop: alias } = target` → `alias === literal` narrows `target` by `prop`
    ///
    /// Returns `(path, literal_type, is_optional, base)` where `base = target`.
    fn aliased_discriminant(
        &self,
        alias_node: NodeIndex,
        literal_node: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, TypeId, bool, NodeIndex)> {
        let node_data = self.arena.get(self.skip_parenthesized(alias_node))?;
        if node_data.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let literal = self.literal_type_from_node(literal_node)?;

        // Case 1: Simple const alias `const alias = target.prop` (or deeper: target.a.b)
        // Resolve alias to its property access initializer, then compute relative path.
        if let Some((_, initializer)) = self.const_condition_initializer(alias_node) {
            let init_expr = self.skip_parenthesized(initializer);
            let init_node = self.arena.get(init_expr)?;
            if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                // Walk from `init_expr` towards the root, collecting segments until we hit `target`.
                if let Some((rel_path, is_optional)) =
                    self.relative_discriminant_path(init_expr, target)
                {
                    return Some((rel_path, literal, is_optional, target));
                }
            }
        }

        // Case 2: Destructuring alias `const { prop: alias } = target`
        if let Some((base, prop_name)) = self.binding_element_property_alias(alias_node)
            && self.is_matching_reference(base, target)
        {
            return Some((vec![prop_name], literal, false, target));
        }

        None
    }

    /// Given a property access `prop_access` (e.g. `this.test.type`) and a target node
    /// (e.g. `this.test`), walk backwards collecting property names until we reach `target`.
    ///
    /// Returns `(relative_path, is_optional)` where `relative_path` is the list of property
    /// names from `target` to `prop_access` (e.g. `["type"]`).
    ///
    /// Returns `None` if `target` is not found in the access chain.
    fn relative_discriminant_path(
        &self,
        prop_access: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, bool)> {
        let mut path: Vec<Atom> = Vec::new();
        let mut is_optional = false;
        let mut current = self.skip_parenthesized(prop_access);

        loop {
            let current_node = self.arena.get(current)?;
            let access = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || current_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                self.arena.get_access_expr(current_node)?
            } else {
                // Reached a non-access node without finding target
                return None;
            };

            if access.question_dot_token {
                is_optional = true;
            }

            let prop_name = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let ident = self.arena.get_identifier_at(access.name_or_argument)?;
                self.interner.intern_string(&ident.escaped_text)
            } else {
                self.literal_atom_from_node_or_type(access.name_or_argument)?
            };

            // This is the prop name at the current level; push it (path is built backwards)
            path.push(prop_name);

            // Move to the base of this access
            let base_expr = self.skip_parenthesized(access.expression);

            // Check if the base matches the target
            if self.is_matching_reference(base_expr, target) {
                // Found! Reverse path to get correct order.
                path.reverse();
                return Some((path, is_optional));
            }

            current = base_expr;
        }
    }

    pub(crate) fn literal_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_matching_reference(left, target) {
            return self.literal_type_from_node(right);
        }
        if self.is_matching_reference(right, target) {
            return self.literal_type_from_node(left);
        }
        None
    }
}
