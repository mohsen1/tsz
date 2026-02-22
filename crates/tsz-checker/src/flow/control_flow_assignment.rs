//! Assignment type resolution and destructuring matching for `FlowAnalyzer`.
//!
//! Condition-based narrowing (switch clauses, binary/logical expressions, typeof/instanceof)
//! has been extracted to `control_flow_condition_narrowing.rs`.

use crate::control_flow::{FlowAnalyzer, PropertyKey};
use crate::query_boundaries::flow_analysis::{
    are_types_mutually_subtype, fallback_compound_assignment_result, get_array_element_type,
    is_compound_assignment_operator, map_compound_assignment_to_binary, union_members_for_type,
    widen_literal_to_primitive,
};
use tsz_common::interner::Atom;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn get_assigned_type(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
        widen_literals_for_destructuring: bool,
    ) -> Option<TypeId> {
        let node = self.arena.get(assignment_node)?;

        // CRITICAL FIX: Handle compound assignments (+=, -=, *=, etc.)
        // Compound assignments compute the result of a binary operation and assign it back.
        // Example: x += 1 where x: string | number should narrow x to number after assignment.
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            // Check if this is an assignment to our target reference
            if self.is_matching_reference(bin.left, target) {
                // Check if this is a compound assignment operator (not simple =)
                if bin.operator_token != SyntaxKind::EqualsToken as u16
                    && is_compound_assignment_operator(bin.operator_token)
                {
                    use tsz_solver::{BinaryOpEvaluator, BinaryOpResult};

                    // When node_types is not available, use heuristics for flow narrowing
                    if self.node_types.is_none() {
                        return fallback_compound_assignment_result(
                            self.interner,
                            bin.operator_token,
                            self.literal_type_from_node(bin.right),
                        );
                    }

                    if bin.operator_token == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                        || bin.operator_token == SyntaxKind::BarBarEqualsToken as u16
                        || bin.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16
                    {
                        if let Some(node_types) = self.node_types
                            && let Some(&rhs_type) = node_types.get(&bin.right.0)
                        {
                            return Some(rhs_type);
                        }
                        return None;
                    }

                    // Get LHS type (current narrowed type of the variable)
                    let left_type = if let Some(node_types) = self.node_types
                        && let Some(&lhs_type) = node_types.get(&bin.left.0)
                    {
                        lhs_type
                    } else {
                        // Fall back - shouldn't happen due to the check above
                        return None;
                    };

                    // Get RHS type
                    let right_type = if let Some(node_types) = self.node_types
                        && let Some(&rhs_type) = node_types.get(&bin.right.0)
                    {
                        rhs_type
                    } else {
                        // Fall back - shouldn't happen due to the check above
                        return None;
                    };

                    // Map compound assignment operator to binary operator
                    let op_str = map_compound_assignment_to_binary(bin.operator_token)?;

                    // Evaluate the binary operation to get result type
                    let evaluator = BinaryOpEvaluator::new(self.interner);
                    return match evaluator.evaluate(left_type, right_type, op_str) {
                        BinaryOpResult::Success(result) => Some(result),
                        // For type errors, return ANY to prevent cascading errors
                        BinaryOpResult::TypeError { .. } => Some(TypeId::ANY),
                    };
                }
            }
        }

        if let Some(rhs) = self.assignment_rhs_for_reference(assignment_node, target) {
            // Unannotated declaration initializers `let/var/const x = []` should flow as
            // evolving-any arrays so immediate writes like `x.push(...)` are permitted.
            // Keep this scoped to declaration assignments to avoid changing expression-level
            // `[]` behavior (e.g. generic inference with `id([])`).
            let is_unannotated_decl_init = (self
                .is_mutable_var_decl_without_annotation(assignment_node)
                || (self.is_const_variable_declaration(assignment_node)
                    && !self.is_var_decl_with_type_annotation(assignment_node)))
                && self.arena.get(rhs).is_some_and(|rhs_node| {
                    rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        && self
                            .arena
                            .get_literal_expr(rhs_node)
                            .is_some_and(|lit| lit.elements.nodes.is_empty())
                });
            if is_unannotated_decl_init {
                return Some(self.interner.array(TypeId::ANY));
            }

            // For flow narrowing, prefer literal types from AST nodes over the type checker's widened types
            // This ensures that `x = 42` narrows to literal 42.0, not just NUMBER
            // This matches TypeScript's behavior where control flow analysis preserves literal types
            if let Some(literal_type) = self.literal_type_from_node(rhs) {
                // For destructuring contexts, widen literals to primitives to match TypeScript
                // Example: [x] = [1] widens to number, ({ x } = { x: 1 }) widens to number
                // Also handles default values: [x = 2] = [] widens to number
                if widen_literals_for_destructuring {
                    return Some(widen_literal_to_primitive(self.interner, literal_type));
                }
                // For mutable variable declarations (let/var) without type annotations,
                // widen literal types to their base types to match TypeScript behavior.
                // Example: let x = "hi" -> string (not "hi"), let x = 42 -> number (not 42)
                if self.is_mutable_var_decl_without_annotation(assignment_node) {
                    return Some(widen_literal_to_primitive(self.interner, literal_type));
                }
                // For const variable declarations with type annotations, if the literal
                // type (null or undefined) is not assignable to the declared annotation
                // type, return None (use the declared type). This prevents TS18047 false
                // positives like `const x: IAsyncEnumerator<number> = null` where
                // subsequent uses of x should see IAsyncEnumerator<number>, not null.
                // Note: annotation type is keyed by annotation node index in node_types,
                // not by the variable declaration node index.
                // We use strict null check mode (flags=1) because the question is whether
                // null semantically belongs to the declared type — a strict-mode concept.
                if (literal_type == TypeId::NULL || literal_type == TypeId::UNDEFINED)
                    && let Some(annotation_type) =
                        self.annotation_type_from_var_decl_node(assignment_node)
                    && !self.is_assignable_to_strict_null(literal_type, annotation_type)
                {
                    return None;
                }
                return Some(literal_type);
            }
            // For variable declarations with type annotations, preserve the declared
            // type (return None) in two cases:
            //
            // 1. Non-const (let/var) declarations: the declared type is the authoritative
            //    flow type. E.g., `let x: string | number = "hello"` has flow type
            //    `string | number`, not `"hello"`. Critical for loop fixed-point analysis.
            //
            // 2. Object/array literal initializers (even const): the structural type
            //    loses optional properties and interface/readonly modifiers.
            //    E.g., `const xs: readonly number[] = [1, 2]` must keep `readonly number[]`.
            if self.is_var_decl_with_type_annotation(assignment_node) {
                let is_const = self.is_const_variable_declaration(assignment_node);
                let is_structural_literal = self.arena.get(rhs).is_some_and(|rhs_node| {
                    rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                });
                if !is_const || is_structural_literal {
                    return None;
                }
            }
            // `undefined` identifier (not a keyword) as initializer: if the variable
            // has a type annotation that doesn't include undefined, use the declared type.
            if let Some(nullish_type) = self.nullish_literal_type(rhs) {
                if let Some(annotation_type) =
                    self.annotation_type_from_var_decl_node(assignment_node)
                    && !self.is_assignable_to_strict_null(nullish_type, annotation_type)
                {
                    return None;
                }
                return Some(nullish_type);
            }
            if let Some(node_types) = self.node_types
                && let Some(&rhs_type) = node_types.get(&rhs.0)
            {
                // Only apply assignment-based "killing definition" narrowing when
                // the write itself is compatible. For invalid assignments, TypeScript
                // reports the assignment error but keeps subsequent reads at the
                // variable's declared type.
                if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && let Some(bin) = self.arena.get_binary_expr(node)
                    && bin.operator_token == SyntaxKind::EqualsToken as u16
                    && self.is_matching_reference(bin.left, target)
                {
                    let declared_target_type = self
                        .binder
                        .resolve_identifier(self.arena, bin.left)
                        .and_then(|sym| self.binder.get_symbol(sym))
                        .map(|sym| sym.value_declaration)
                        .filter(|decl| decl.is_some())
                        .and_then(|decl| node_types.get(&decl.0).copied())
                        .or_else(|| node_types.get(&bin.left.0).copied());

                    if let Some(lhs_type) = declared_target_type
                        && !self.is_assignable_to(rhs_type, lhs_type)
                    {
                        return None;
                    }
                }
                return Some(rhs_type);
            }
            return None;
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            let unary = self.arena.get_unary_expr(node)?;
            if (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
                && self.is_matching_reference(unary.operand, target)
            {
                return Some(TypeId::NUMBER);
            }
        }

        // For for-of / for-in initializer assignments, the binder creates a
        // flow ASSIGNMENT pointing to the initializer node (e.g. the identifier
        // `x` in `for (x of arr)`). Walk up to find the parent for-of/for-in
        // statement and get the iterated expression's element type.
        if self.is_matching_reference(assignment_node, target)
            && let Some(ext) = self.arena.get_extended(assignment_node)
            && ext.parent.is_some()
            && let Some(parent_node) = self.arena.get(ext.parent)
            && (parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                || parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT)
            && let Some(for_data) = self.arena.get_for_in_of(parent_node)
            && let Some(node_types) = self.node_types
            && let Some(&expr_type) = node_types.get(&for_data.expression.0)
        {
            if parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT {
                return Some(TypeId::STRING);
            }
            // for-of: extract element type from the array/iterable expression type
            if let Some(elem) = get_array_element_type(self.interner, expr_type) {
                return Some(elem);
            }
        }

        None
    }

    pub(crate) fn assignment_rhs_for_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.arena.get(assignment_node)?;

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                if self.is_matching_reference(bin.left, reference) {
                    return Some(bin.right);
                }
                if let Some(rhs) = self.match_destructuring_rhs(bin.left, bin.right, reference) {
                    return Some(rhs);
                }
            }
            return None;
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(node)?;
            if self.is_matching_reference(decl.name, reference) && decl.initializer.is_some() {
                return Some(decl.initializer);
            }
            if decl.initializer.is_some()
                && let Some(rhs) =
                    self.match_destructuring_rhs(decl.name, decl.initializer, reference)
            {
                return Some(rhs);
            }
            return None;
        }

        if (node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node.kind == syntax_kind_ext::VARIABLE_STATEMENT)
            && let Some(list) = self.arena.get_variable(node)
        {
            for &decl_idx in &list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if self.is_matching_reference(decl.name, reference) && decl.initializer.is_some() {
                    return Some(decl.initializer);
                }
                if decl.initializer.is_some()
                    && let Some(rhs) =
                        self.match_destructuring_rhs(decl.name, decl.initializer, reference)
                {
                    return Some(rhs);
                }
            }
        }

        None
    }

    pub(crate) fn match_destructuring_rhs(
        &self,
        pattern: NodeIndex,
        rhs: NodeIndex,
        target: NodeIndex,
    ) -> Option<NodeIndex> {
        if pattern.is_none() {
            return None;
        }

        let pattern = self.skip_parens_and_assertions(pattern);
        let rhs = if rhs.is_none() {
            rhs
        } else {
            self.skip_parens_and_assertions(rhs)
        };

        if rhs.is_some() && self.is_matching_reference(pattern, target) {
            return Some(rhs);
        }

        // Handle default values: when RHS is empty, check for default value in pattern element
        if rhs.is_none()
            && self.assignment_targets_reference_internal(pattern, target)
            && let Some(binding) = self.arena.get_binding_element_at(pattern)
            && binding.initializer.is_some()
        {
            return Some(binding.initializer);
        }

        let node = self.arena.get(pattern)?;
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                    return None;
                }
                if let Some(found) = self.match_destructuring_rhs(bin.left, rhs, target) {
                    return Some(found);
                }
                if self.assignment_targets_reference_internal(bin.left, target) {
                    if let Some(found) = self.match_destructuring_rhs(bin.left, bin.right, target) {
                        return Some(found);
                    }
                    return Some(bin.right);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                // FIX: Array destructuring should return the matching RHS element
                // After [x] = [1] where x: string | number, TypeScript produces `number` (widened primitive)
                // We return the element node here; get_assigned_type handles the widening
                let elements = if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };

                // Get elements from the RHS array literal
                let rhs_elements = self.array_literal_elements(rhs);

                for (i, &elem) in elements.nodes.iter().enumerate() {
                    if elem.is_none() {
                        continue;
                    }

                    // Check if this specific element (or its children) targets our reference
                    if self.assignment_targets_reference_internal(elem, target) {
                        let rhs_elem = rhs_elements
                            .and_then(|re| re.nodes.get(i).copied())
                            .unwrap_or(NodeIndex::NONE);

                        // Recurse to handle nested destructuring: [[x]] = [[1]]
                        return self.match_destructuring_rhs(elem, rhs_elem, target);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN =>
            {
                let elements = if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };
                for &elem in &elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if let Some(found) = self.match_object_pattern_element(elem, rhs, target) {
                        return Some(found);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                let binding = self.arena.get_binding_element(node)?;
                if self.assignment_targets_reference_internal(binding.name, target) {
                    if rhs.is_some() {
                        if let Some(found) = self.match_destructuring_rhs(binding.name, rhs, target)
                        {
                            return Some(found);
                        }
                        if self.is_matching_reference(binding.name, target) {
                            return Some(rhs);
                        }
                    }
                    if binding.initializer.is_some() {
                        if let Some(found) =
                            self.match_destructuring_rhs(binding.name, binding.initializer, target)
                        {
                            return Some(found);
                        }
                        return Some(binding.initializer);
                    }
                }
            }
            _ => {}
        }

        None
    }

    pub(crate) fn match_object_pattern_element(
        &self,
        elem: NodeIndex,
        rhs: NodeIndex,
        target: NodeIndex,
    ) -> Option<NodeIndex> {
        let elem_node = self.arena.get(elem)?;
        match elem_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(elem_node)?;
                if !self.assignment_targets_reference_internal(prop.initializer, target) {
                    return None;
                }
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, prop.name) {
                    if let Some(found) =
                        self.match_destructuring_rhs(prop.initializer, rhs_value, target)
                    {
                        return Some(found);
                    }
                    return Some(rhs_value);
                }
                return self.match_destructuring_rhs(prop.initializer, NodeIndex::NONE, target);
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(elem_node)?;
                if !self.assignment_targets_reference_internal(prop.name, target) {
                    return None;
                }
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, prop.name) {
                    return Some(rhs_value);
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                let binding = self.arena.get_binding_element(elem_node)?;
                if !self.assignment_targets_reference_internal(binding.name, target) {
                    return None;
                }
                let name_idx = if binding.property_name.is_none() {
                    binding.name
                } else {
                    binding.property_name
                };
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, name_idx) {
                    if let Some(found) =
                        self.match_destructuring_rhs(binding.name, rhs_value, target)
                    {
                        return Some(found);
                    }
                    return Some(rhs_value);
                }
                if binding.initializer.is_some() {
                    if let Some(found) =
                        self.match_destructuring_rhs(binding.name, binding.initializer, target)
                    {
                        return Some(found);
                    }
                    return Some(binding.initializer);
                }
            }
            _ => {}
        }
        None
    }

    pub(crate) fn array_literal_elements(&self, rhs: NodeIndex) -> Option<&NodeList> {
        if rhs.is_none() {
            return None;
        }
        let rhs = self.skip_parens_and_assertions(rhs);
        let node = self.arena.get(rhs)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        self.arena.get_literal_expr(node).map(|lit| &lit.elements)
    }

    pub(crate) fn lookup_property_in_rhs(
        &self,
        rhs: NodeIndex,
        name: NodeIndex,
    ) -> Option<NodeIndex> {
        if rhs.is_none() || name.is_none() {
            return None;
        }
        let rhs = self.skip_parens_and_assertions(rhs);
        let rhs_node = self.arena.get(rhs)?;
        let key = self
            .property_key_from_name(name)
            .or_else(|| self.property_key_from_name_with_rhs_effects(name, rhs))?;

        if rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let lit = self.arena.get_literal_expr(rhs_node)?;
            if let PropertyKey::Index(index) = key {
                return lit
                    .elements
                    .nodes
                    .get(index)
                    .copied()
                    .filter(|n| n.is_some());
            }
            return None;
        }

        if rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let lit = self.arena.get_literal_expr(rhs_node)?;
            if let PropertyKey::Atom(atom) = key {
                return self.find_property_in_object_literal(lit, atom);
            }
        }

        None
    }

    fn property_key_from_name_with_rhs_effects(
        &self,
        name: NodeIndex,
        rhs: NodeIndex,
    ) -> Option<PropertyKey> {
        let name = self.skip_parens_and_assertions(name);
        let name_node = self.arena.get(name)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.arena.get_computed_property(name_node)?;
        let key_expr = self.skip_parens_and_assertions(computed.expression);

        if let Some(key) = self.property_key_from_assignment_like_expr(key_expr) {
            return Some(key);
        }

        let key_node = self.arena.get(key_expr)?;
        if key_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        self.property_key_from_rhs_assignment_to_reference(rhs, key_expr)
    }

    fn property_key_from_assignment_like_expr(&self, expr: NodeIndex) -> Option<PropertyKey> {
        let expr = self.skip_parens_and_assertions(expr);
        let node = self.arena.get(expr)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }

        if let Some(value) = self.literal_number_from_node_or_type(bin.right)
            && value.fract() == 0.0
            && value >= 0.0
        {
            return Some(PropertyKey::Index(value as usize));
        }
        self.literal_atom_from_node_or_type(bin.right)
            .map(PropertyKey::Atom)
    }

    fn property_key_from_rhs_assignment_to_reference(
        &self,
        rhs: NodeIndex,
        reference: NodeIndex,
    ) -> Option<PropertyKey> {
        let rhs = self.skip_parens_and_assertions(rhs);
        let rhs_node = self.arena.get(rhs)?;
        if rhs_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let rhs_elements = self.arena.get_literal_expr(rhs_node)?;
        let mut inferred = None;
        for &elem in &rhs_elements.elements.nodes {
            if elem.is_none() {
                continue;
            }
            if let Some(key) = self.property_key_from_assignment_to_reference(elem, reference) {
                inferred = Some(key);
            }
        }
        inferred
    }

    fn property_key_from_assignment_to_reference(
        &self,
        expr: NodeIndex,
        reference: NodeIndex,
    ) -> Option<PropertyKey> {
        let expr = self.skip_parens_and_assertions(expr);
        let node = self.arena.get(expr)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        if !self.is_matching_reference(bin.left, reference) {
            return None;
        }
        if let Some(value) = self.literal_number_from_node_or_type(bin.right)
            && value.fract() == 0.0
            && value >= 0.0
        {
            return Some(PropertyKey::Index(value as usize));
        }
        self.literal_atom_from_node_or_type(bin.right)
            .map(PropertyKey::Atom)
    }

    pub(crate) fn find_property_in_object_literal(
        &self,
        literal: &tsz_parser::parser::node::LiteralExprData,
        target: Atom,
    ) -> Option<NodeIndex> {
        for &elem in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem) else {
                continue;
            };
            match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_property_assignment(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target
                    {
                        return Some(prop.initializer);
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_shorthand_property(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target
                    {
                        return Some(prop.name);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn assignment_affects_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self.arena.get_binary_expr(node).is_some_and(|bin| {
                self.is_assignment_operator(bin.operator_token)
                    && self.assignment_affects_reference(bin.left, target)
            });
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self.arena.get_unary_expr(node).is_some_and(|unary| {
                (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16)
                    && self.assignment_affects_reference(unary.operand, target)
            });
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .is_some_and(|decl| self.assignment_affects_reference(decl.name, target));
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_affects_reference(decl.name, target)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        self.assignment_affects_reference(assignment_node, target)
    }

    pub fn assignment_targets_reference(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        self.assignment_targets_reference_node(assignment_node, target)
    }

    pub(crate) fn assignment_targets_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self.arena.get_binary_expr(node).is_some_and(|bin| {
                let is_op = self.is_assignment_operator(bin.operator_token);
                let targets = self.assignment_targets_reference_internal(bin.left, target);
                is_op && targets
            });
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self.arena.get_unary_expr(node).is_some_and(|unary| {
                (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16)
                    && self.assignment_targets_reference_internal(unary.operand, target)
            });
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .is_some_and(|decl| self.assignment_targets_reference_internal(decl.name, target));
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_targets_reference_internal(decl.name, target)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        self.assignment_targets_reference_internal(assignment_node, target)
    }

    /// Check if the assignment node reassigns a BASE of the reference.
    ///
    /// For example, if `reference` is `obj.prop` and the assignment is `obj = { prop: 1 }`,
    /// this returns true because `obj` (a base of `obj.prop`) is being reassigned.
    ///
    /// But if `reference` is `config['works']` and the assignment is `config.works.prop = 'test'`,
    /// this returns false because the LHS is deeper than the reference, not a base of it.
    pub(crate) fn assignment_targets_base_of_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> bool {
        // Walk up the bases of the reference and check if the assignment targets any of them
        let mut current = self.reference_base(reference);
        while let Some(base) = current {
            if self.assignment_targets_reference_node(assignment_node, base) {
                return true;
            }
            current = self.reference_base(base);
        }
        false
    }

    pub(crate) const fn is_assignment_operator(&self, operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    pub(crate) fn narrow_assignment(&self, initial_type: TypeId, assigned_type: TypeId) -> TypeId {
        if initial_type == TypeId::ANY
            || initial_type == TypeId::ERROR
            || initial_type == TypeId::UNKNOWN
        {
            return initial_type;
        }

        let members_opt = union_members_for_type(self.interner, initial_type);
        let members = match members_opt {
            Some(m) => m,
            None => return initial_type,
        };

        if members.len() <= 1 {
            return initial_type;
        }

        let mut kept = Vec::new();
        for &m in &members {
            if are_types_mutually_subtype(self.interner, assigned_type, m) {
                kept.push(m);
            }
        }

        if kept.is_empty() {
            initial_type
        } else if kept.len() == 1 {
            kept[0]
        } else {
            self.interner.union(kept)
        }
    }
}
