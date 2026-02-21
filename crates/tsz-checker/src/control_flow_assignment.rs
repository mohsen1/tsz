//! Assignment type resolution, destructuring matching, and condition-based type narrowing
//! for `FlowAnalyzer`.

use crate::control_flow::{FlowAnalyzer, PropertyKey};
use crate::query_boundaries::flow_analysis::{
    fallback_compound_assignment_result, get_array_element_type, is_compound_assignment_operator,
    is_unit_type, map_compound_assignment_to_binary, widen_literal_to_primitive,
};
use tsz_binder::{FlowNodeId, SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::BinaryExprData;
use tsz_parser::parser::{NodeIndex, NodeList, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{NarrowingContext, TypeGuard, TypeId, TypeofKind};

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

    pub(crate) fn narrow_by_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let binary = BinaryExprData {
            left: switch_expr,
            operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
            right: case_expr,
        };

        self.narrow_by_binary_expr(type_id, &binary, target, true, narrowing, FlowNodeId::NONE)
    }

    pub(crate) fn narrow_by_default_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_block: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let Some(case_block_node) = self.arena.get(case_block) else {
            return type_id;
        };
        let Some(case_block) = self.arena.get_block(case_block_node) else {
            return type_id;
        };

        // Fast path: if this switch does not reference the target (directly or via discriminant
        // property access like switch(x.kind) when narrowing x), it cannot affect target's type.
        let target_is_switch_expr = self.is_matching_reference(switch_expr, target);
        let mut discriminant_info = None;

        if !target_is_switch_expr {
            discriminant_info = self.discriminant_property_info(switch_expr, target);
            let switch_targets_base = discriminant_info
                .as_ref()
                .is_some_and(|(_, _, base)| self.is_matching_reference(*base, target));
            if !switch_targets_base {
                return type_id;
            }
        }

        // Excluding finitely many case literals from broad primitive domains does not narrow.
        // Example: number minus {0, 1, 2, ...} is still number.
        if target_is_switch_expr
            && matches!(
                type_id,
                TypeId::NUMBER | TypeId::STRING | TypeId::BIGINT | TypeId::SYMBOL | TypeId::OBJECT
            )
        {
            return type_id;
        }

        // OPTIMIZATION: For direct switches on the target (switch(x) {...}) OR discriminant switches (switch(x.kind)),
        // collect all case types first and exclude them in a single O(N) pass.
        // This avoids O(N²) behavior when there are many case clauses.
        if target_is_switch_expr || discriminant_info.is_some() {
            // Collect all case expression types
            let mut excluded_types: Vec<TypeId> = Vec::new();
            for &clause_idx in &case_block.statements.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(clause) = self.arena.get_case_clause(clause_node) else {
                    continue;
                };
                if clause.expression.is_none() {
                    continue; // Skip default clause
                }

                // Try to get the type of the case expression
                // First try literal extraction (fast path for constants)
                if let Some(lit_type) = self.literal_type_from_node(clause.expression) {
                    excluded_types.push(lit_type);
                } else if let Some(node_types) = self.node_types {
                    // Fall back to computed node types
                    if let Some(&expr_type) = node_types.get(&clause.expression.0) {
                        excluded_types.push(expr_type);
                    }
                }
            }

            if !excluded_types.is_empty() {
                if target_is_switch_expr {
                    // Use batched narrowing for O(N) instead of O(N²)
                    return narrowing.narrow_excluding_types(type_id, &excluded_types);
                } else if let Some((path, _, _)) = discriminant_info {
                    // Use batched discriminant narrowing
                    return narrowing.narrow_by_excluding_discriminant_values(
                        type_id,
                        &path,
                        &excluded_types,
                    );
                }
            }
        }

        // Fall back to sequential narrowing for complex cases
        // (e.g., switch(x.kind) where we need property-based narrowing)
        let mut narrowed = type_id;
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                continue;
            };
            if clause.expression.is_none() {
                continue;
            }

            let binary = BinaryExprData {
                left: switch_expr,
                operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
                right: clause.expression,
            };
            narrowed = self.narrow_by_binary_expr(
                narrowed,
                &binary,
                target,
                false,
                narrowing,
                FlowNodeId::NONE,
            );
        }

        narrowed
    }

    /// Apply type narrowing based on a condition expression.
    pub(crate) fn narrow_type_by_condition(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        let mut visited_aliases = Vec::new();

        self.narrow_type_by_condition_inner(
            type_id,
            condition_idx,
            target,
            is_true_branch,
            antecedent_id,
            &mut visited_aliases,
        )
    }

    pub(crate) fn narrow_type_by_condition_inner(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> TypeId {
        let condition_idx = self.skip_parenthesized(condition_idx);
        let Some(cond_node) = self.arena.get(condition_idx) else {
            return type_id;
        };

        // Fast path: most binary operators never contribute to flow narrowing.
        // Skip context setup and guard extraction for those operators.
        if cond_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(cond_node)
            && !matches!(
                bin.operator_token,
                k if k == SyntaxKind::AmpersandAmpersandToken as u16
                    || k == SyntaxKind::BarBarToken as u16
                    || k == SyntaxKind::QuestionQuestionToken as u16
                    || k == SyntaxKind::EqualsToken as u16
                    || k == SyntaxKind::InstanceOfKeyword as u16
                    || k == SyntaxKind::InKeyword as u16
                    || k == SyntaxKind::EqualsEqualsEqualsToken as u16
                    || k == SyntaxKind::ExclamationEqualsEqualsToken as u16
                    || k == SyntaxKind::EqualsEqualsToken as u16
                    || k == SyntaxKind::ExclamationEqualsToken as u16
            )
        {
            return type_id;
        }

        // Create narrowing context and wire up TypeEnvironment if available
        // This enables proper resolution of Lazy types (type aliases) during narrowing
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
        } else {
            NarrowingContext::new(self.interner)
        };

        if cond_node.kind == SyntaxKind::Identifier as u16
            && let Some((sym_id, initializer)) = self.const_condition_initializer(condition_idx)
            && !visited_aliases.contains(&sym_id)
        {
            visited_aliases.push(sym_id);
            let narrowed = self.narrow_type_by_condition_inner(
                type_id,
                initializer,
                target,
                is_true_branch,
                antecedent_id,
                visited_aliases,
            );
            visited_aliases.pop();
            return narrowed;
        }

        match cond_node.kind {
            // typeof x === "string", x instanceof Class, "prop" in x, etc.
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(cond_node) {
                    // Handle logical operators (&&, ||) with special recursion
                    if let Some(narrowed) = self.narrow_by_logical_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        antecedent_id,
                        visited_aliases,
                    ) {
                        return narrowed;
                    }

                    // Handle boolean comparison: `expr === true`, `expr === false`,
                    // `expr !== true`, `expr !== false`, and reversed variants.
                    // TypeScript treats comparing a type guard result to true/false as
                    // preserving/inverting the type guard:
                    //   if (x instanceof Error === false) { ... }
                    //   if (isString(x) === true) { ... }
                    if let Some(narrowed) = self.narrow_by_boolean_comparison(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        antecedent_id,
                        visited_aliases,
                    ) {
                        return narrowed;
                    }

                    // Fast-path: avoid expensive generic guard extraction when the
                    // comparison does not directly target this reference.
                    //
                    // Example hot path:
                    //   if (e.kind === "type42") { ... } while narrowing `e`
                    //
                    // `extract_type_guard` first targets `e.kind`, which won't match `e`,
                    // then we still do full binary narrowing below. Skip the extraction in
                    // that common mismatch case and go straight to `narrow_by_binary_expr`.
                    let maybe_direct_guard_target = self.is_matching_reference(bin.left, target)
                        || self.is_matching_reference(bin.right, target)
                        || self.is_typeof_target(bin.left, target)
                        || self.is_typeof_target(bin.right, target);

                    // CRITICAL: Use Solver-First architecture for direct binary guards
                    // when the guard target can actually match our reference.
                    if maybe_direct_guard_target
                        && let Some((guard, guard_target, _is_optional)) =
                            self.extract_type_guard(condition_idx)
                    {
                        // Check if the guard applies to our target reference
                        if self.is_matching_reference(guard_target, target) {
                            // CRITICAL: Invert sense for inequality operators (!== and !=)
                            // This applies to ALL guards, not just typeof
                            // For `x !== "string"` or `x.kind !== "circle"`, the true branch should EXCLUDE
                            let effective_sense = if bin.operator_token
                                == SyntaxKind::ExclamationEqualsEqualsToken as u16
                                || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16
                            {
                                !is_true_branch
                            } else {
                                is_true_branch
                            };
                            // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                            return narrowing.narrow_type(type_id, &guard, effective_sense);
                        }
                    }

                    // CRITICAL: Try bidirectional narrowing for x === y where both are references
                    // This handles cases that don't match traditional type guard patterns
                    // Example: if (x === y) { x } should narrow x based on y's type
                    let narrowed = self.narrow_by_binary_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        &narrowing,
                        antecedent_id,
                    );
                    return narrowed;
                }
            }

            // User-defined type guards: isString(x), obj.isString(), assertsIs(x), etc.
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                // CRITICAL: Use Solver-First architecture for call expressions
                // Extract TypeGuard from AST (Checker responsibility: WHERE + WHAT)
                if let Some((guard, guard_target, is_optional)) =
                    self.extract_type_guard(condition_idx)
                {
                    // CRITICAL: Optional chaining behavior
                    // If call is optional (obj?.method(x)), only narrow the true branch
                    // The false branch might mean the method wasn't called (obj was nullish)
                    if is_optional && !is_true_branch {
                        return type_id;
                    }

                    // Check if the guard applies to our target reference
                    if self.is_matching_reference(guard_target, target) {
                        use tracing::trace;
                        trace!(
                            ?guard,
                            ?type_id,
                            ?is_true_branch,
                            "Applying guard from call expression"
                        );
                        // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                        let result = narrowing.narrow_type(type_id, &guard, is_true_branch);
                        trace!(?result, "Guard application result");
                        return result;
                    }
                }

                return type_id;
            }

            // Prefix unary: !x
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(cond_node) {
                    // !x inverts the narrowing
                    if unary.operator == SyntaxKind::ExclamationToken as u16 {
                        return self.narrow_type_by_condition_inner(
                            type_id,
                            unary.operand,
                            target,
                            !is_true_branch,
                            antecedent_id,
                            visited_aliases,
                        );
                    }
                }
            }

            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(cond_node) {
                    if let Some(narrowed) =
                        self.narrow_by_call_predicate(type_id, call, target, is_true_branch)
                    {
                        return narrowed;
                    }
                    if is_true_branch {
                        let optional_call =
                            (cond_node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0;
                        if optional_call && self.is_matching_reference(call.expression, target) {
                            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        }
                        if let Some(callee_node) = self.arena.get(call.expression)
                            && let Some(access) = self.arena.get_access_expr(callee_node)
                            && access.question_dot_token
                            && self.is_matching_reference(access.expression, target)
                        {
                            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        }
                    }
                }
            }

            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(cond_node) {
                    // Handle optional chaining: y?.a
                    if access.question_dot_token
                        && is_true_branch
                        && self.is_matching_reference(access.expression, target)
                    {
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
                    }
                }
                // Handle discriminant narrowing (discriminated unions)
                // For `if (x.flag)` where x is a discriminated union like
                // `{flag: true; data: string} | {flag: false; data: number}`,
                // narrow x by discriminant `flag === true`.
                // BUT: if the result is `never`, the type isn't actually a
                // discriminated union — fall through to truthiness narrowing.
                if let Some(property_path) = self.discriminant_property(condition_idx, target) {
                    let literal_true = self.interner.literal_boolean(true);
                    let narrowed = if is_true_branch {
                        narrowing.narrow_by_discriminant(type_id, &property_path, literal_true)
                    } else {
                        narrowing.narrow_by_excluding_discriminant(
                            type_id,
                            &property_path,
                            literal_true,
                        )
                    };
                    if narrowed != TypeId::NEVER {
                        return narrowed;
                    }
                    // Fall through: not a real discriminated union, try truthiness
                }

                // Handle truthiness narrowing for property/element access: if (y.a)
                if self.is_matching_reference(condition_idx, target) {
                    if is_true_branch {
                        // Remove null/undefined (truthy narrowing)
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
                    }
                    // False branch - keep only falsy types (use Solver for NaN handling)
                    return narrowing.narrow_to_falsy(type_id);
                }
            }

            // Truthiness check: if (x)
            // Use Solver-First architecture: delegate to TypeGuard::Truthy
            _ => {
                if self.is_matching_reference(condition_idx, target) {
                    return narrowing.narrow_type(type_id, &TypeGuard::Truthy, is_true_branch);
                }
            }
        }

        type_id
    }

    /// Check if a node is a property access or element access expression.
    ///
    /// This is used to prevent discriminant guards from being applied to property
    /// access results. Discriminant guards (like `obj.kind === "a"`) should only
    /// narrow the base object (`obj`), not property access results (like `obj.value`).
    fn is_property_or_element_access(&self, node: NodeIndex) -> bool {
        let node = self.skip_parenthesized_non_recursive(node);
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        node_data.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node_data.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    /// Skip parentheses (non-recursive to avoid issues with circular references).
    fn skip_parenthesized_non_recursive(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            // Limit iterations to prevent infinite loops
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let Some(paren) = self.arena.get_parenthesized(node) else {
                    return idx;
                };
                idx = paren.expression;
            } else {
                return idx;
            }
        }
        idx
    }

    pub(crate) fn const_condition_initializer(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(SymbolId, NodeIndex)> {
        let sym_id = self.binder.resolve_identifier(self.arena, ident_idx)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return None;
        }
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        if !self.is_const_variable_declaration(decl_idx) {
            return None;
        }
        let decl = self.arena.get_variable_declaration(decl_node)?;
        if decl.initializer.is_none() {
            return None;
        }
        Some((sym_id, decl.initializer))
    }

    pub(crate) fn is_const_variable_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let mut flags = decl_node.flags as u32;
        if (flags & (node_flags::LET | node_flags::CONST)) == 0 {
            let Some(ext) = self.arena.get_extended(decl_idx) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                return false;
            }
            flags |= parent_node.flags as u32;
        }
        (flags & node_flags::CONST) != 0
    }

    /// Check if a symbol is const (immutable) vs mutable (let/var).
    ///
    /// This is used for loop widening: const variables preserve narrowing through loops,
    /// while mutable variables are widened to the declared type to account for mutations.
    pub(crate) fn is_const_symbol(&self, sym_id: SymbolId) -> bool {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let symbol = match self.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return false, // Assume mutable if we can't determine
        };

        // Check the value declaration
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false; // Assume mutable if no declaration
        }

        let decl_node = match self.arena.get(decl_idx) {
            Some(node) => node,
            None => return false,
        };

        // For variable declarations, the CONST flag is on the VARIABLE_DECLARATION_LIST parent
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(ext) = self.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.arena.get(ext.parent)
        {
            let flags = parent_node.flags as u32;
            return (flags & node_flags::CONST) != 0;
        }

        // For other node types, check the node's own flags
        let flags = decl_node.flags as u32;
        (flags & node_flags::CONST) != 0
    }

    /// Narrow type based on a binary expression (===, !==, typeof checks, etc.)
    pub(crate) fn narrow_by_binary_expr(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        let operator = bin.operator_token;

        // Unwrap assignment expressions: if (flag = (x instanceof Foo)) should narrow based on RHS
        // The assignment itself doesn't provide narrowing, but its RHS might
        if operator == SyntaxKind::EqualsToken as u16 {
            if self.arena.get(bin.right).is_some() {
                // Recursively narrow based on the RHS expression
                let mut visited = Vec::new();
                return self.narrow_type_by_condition_inner(
                    type_id,
                    bin.right,
                    target,
                    is_true_branch,
                    antecedent_id,
                    &mut visited,
                );
            }
            return type_id;
        }

        if operator == SyntaxKind::InstanceOfKeyword as u16 {
            return self.narrow_by_instanceof(type_id, bin, target, is_true_branch);
        }

        if operator == SyntaxKind::InKeyword as u16 {
            return self.narrow_by_in_operator(type_id, bin, target, is_true_branch);
        }

        let (is_equals, is_strict) = match operator {
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => (true, true),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => (false, true),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => (true, false),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => (false, false),
            _ => return type_id,
        };

        let effective_truth = if is_equals {
            is_true_branch
        } else {
            !is_true_branch
        };

        if let Some(type_name) = self.typeof_comparison_literal(bin.left, bin.right, target) {
            // Use unified narrow_type API with TypeGuard::Typeof for both branches
            if let Some(typeof_kind) = TypeofKind::parse(type_name) {
                return narrowing.narrow_type(
                    type_id,
                    &TypeGuard::Typeof(typeof_kind),
                    effective_truth,
                );
            }
            // Unknown typeof string (e.g., host-defined types), no narrowing
            return type_id;
        }

        if let Some(nullish) = self.nullish_comparison(bin.left, bin.right, target) {
            if is_strict {
                if effective_truth {
                    return nullish;
                }
                return narrowing.narrow_excluding_type(type_id, nullish);
            }

            let nullish_union = self.interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
            if effective_truth {
                return nullish_union;
            }

            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
        }

        if is_strict {
            if let Some((property_path, literal_type, is_optional, base)) =
                self.discriminant_comparison(bin.left, bin.right, target)
            {
                // Determine whether we should apply discriminant narrowing.
                //
                // Two scenarios for skipping:
                // 1. INDIRECT property access: target is a sub-property of base
                //    (e.g., `if (obj.kind === "a") { obj.kind; }` — target=`obj.kind`, base=`obj`)
                //    Literal comparison handles this; discriminant narrowing would yield NEVER.
                // 2. ALIASED + MUTABLE: target is a let-bound variable with an aliased discriminant
                //    (e.g., aliased condition on a reassignable variable)
                //
                // IMPORTANT: for DIRECT discriminant narrowing where base == target,
                // we MUST allow it even when target is a property access.
                // e.g., `if (this.test.type === "a") { this.test.name; }` — target=`this.test`
                // must be narrowable since base == target == `this.test`.
                let is_aliased_discriminant = !self.is_matching_reference(base, target);
                let is_property_access = self.is_property_or_element_access(target);
                let is_mutable = self.is_mutable_variable(target);

                // Skip only when: (aliased AND (indirect property access OR mutable target))
                // Direct discriminant (is_aliased_discriminant = false) always applies.
                if !(is_aliased_discriminant && (is_property_access || is_mutable)) {
                    let mut base_type = type_id;
                    if is_optional && effective_truth {
                        let narrowed = narrowing.narrow_excluding_type(base_type, TypeId::NULL);
                        base_type = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                    }
                    return narrowing.narrow_by_discriminant_for_type(
                        base_type,
                        &property_path,
                        literal_type,
                        effective_truth,
                    );
                }
                // Skipped: indirect property access or aliased let-bound variable.
                // The type will be computed from the already-narrowed base or via literal comparison.
            }

            if let Some(literal_type) = self.literal_comparison(bin.left, bin.right, target) {
                if effective_truth {
                    let narrowed = narrowing.narrow_to_type(type_id, literal_type);
                    if narrowed != TypeId::NEVER {
                        return narrowed;
                    }
                    if narrowing.literal_assignable_to(literal_type, type_id) {
                        return literal_type;
                    }
                    return TypeId::NEVER;
                }
                return narrowing.narrow_excluding_type(type_id, literal_type);
            }
        }

        // Bidirectional narrowing: x === y where both are references
        // This handles cases like: if (x === y) { ... }
        // where both x and y are variables (not just literals)
        if is_strict {
            // Helper to get flow type of the "other" node
            let get_other_flow_type = |other_node: NodeIndex| -> Option<TypeId> {
                let node_types = self.node_types?;
                let initial_type = *node_types.get(&other_node.0)?;

                // CRITICAL FIX: Use flow analysis if we have a valid flow node
                // This gets the flow-narrowed type of the other reference
                if antecedent_id.is_some() {
                    Some(self.get_flow_type(other_node, initial_type, antecedent_id))
                } else {
                    // Fallback for tests or when no flow context exists
                    Some(initial_type)
                }
            };

            // Check if target is on the left side (x === y, target is x)
            if self.is_matching_reference(bin.left, target) {
                // We need the type of the RIGHT side (y)
                if let Some(right_type) = get_other_flow_type(bin.right) {
                    if effective_truth {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            true,
                        );
                    } else if is_unit_type(self.interner, right_type) {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            false,
                        );
                    }
                }
            }

            // Check if target is on the right side (y === x, target is x)
            if self.is_matching_reference(bin.right, target) {
                // We need the type of the LEFT side (y)
                if let Some(left_type) = get_other_flow_type(bin.left) {
                    if effective_truth {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            true,
                        );
                    } else if is_unit_type(self.interner, left_type) {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            false,
                        );
                    }
                }
            }
        }

        type_id
    }

    /// Handle boolean comparison narrowing: `expr === true`, `expr === false`,
    /// `expr !== true`, `expr !== false`, and their reversed variants.
    ///
    /// When a type guard expression is compared to `true` or `false`, TypeScript
    /// preserves the narrowing. For example:
    ///   - `x instanceof Error === false` → same as `!(x instanceof Error)`
    ///   - `isString(x) === true` → same as `isString(x)`
    ///   - `x instanceof Error !== false` → same as `x instanceof Error`
    fn narrow_by_boolean_comparison(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        // Only handle strict/loose equality/inequality operators
        let is_strict_eq = bin.operator_token == SyntaxKind::EqualsEqualsEqualsToken as u16;
        let is_strict_neq = bin.operator_token == SyntaxKind::ExclamationEqualsEqualsToken as u16;
        let is_loose_eq = bin.operator_token == SyntaxKind::EqualsEqualsToken as u16;
        let is_loose_neq = bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16;

        if !is_strict_eq && !is_strict_neq && !is_loose_eq && !is_loose_neq {
            return None;
        }

        // Check for true/false on either side
        let (guard_expr, is_compared_to_true) = if self.is_boolean_literal(bin.right) {
            (bin.left, self.is_true_literal(bin.right))
        } else if self.is_boolean_literal(bin.left) {
            (bin.right, self.is_true_literal(bin.left))
        } else {
            return None;
        };

        // Determine effective sense:
        // `expr === true` in true branch → narrow as if expr is true
        // `expr === false` in true branch → narrow as if expr is false
        // `expr !== true` in true branch → narrow as if expr is false
        // `expr !== false` in true branch → narrow as if expr is true
        let is_negated = is_strict_neq || is_loose_neq;
        let effective_sense = if is_compared_to_true {
            if is_negated {
                !is_true_branch
            } else {
                is_true_branch
            }
        } else {
            // compared to false — invert
            if is_negated {
                is_true_branch
            } else {
                !is_true_branch
            }
        };

        // Recursively narrow based on the guard expression
        Some(self.narrow_type_by_condition_inner(
            type_id,
            guard_expr,
            target,
            effective_sense,
            antecedent_id,
            visited_aliases,
        ))
    }

    /// Check if a node is the literal `true` or `false`.
    fn is_boolean_literal(&self, node: NodeIndex) -> bool {
        let node = self.skip_parenthesized(node);
        self.arena.get(node).is_some_and(|n| {
            n.kind == SyntaxKind::TrueKeyword as u16 || n.kind == SyntaxKind::FalseKeyword as u16
        })
    }

    /// Check if a node is the literal `true`.
    fn is_true_literal(&self, node: NodeIndex) -> bool {
        let node = self.skip_parenthesized(node);
        self.arena
            .get(node)
            .is_some_and(|n| n.kind == SyntaxKind::TrueKeyword as u16)
    }

    pub(crate) fn narrow_by_logical_expr(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        let operator = bin.operator_token;

        if operator == SyntaxKind::AmpersandAmpersandToken as u16 {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_true,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                return Some(right_true);
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            let left_true = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                true,
                antecedent_id,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_true,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            return Some(tsz_solver::utils::union_or_single(
                self.interner,
                vec![left_false, right_false],
            ));
        }

        if operator == SyntaxKind::BarBarToken as u16 {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                let left_false = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    false,
                    antecedent_id,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_false,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                return Some(tsz_solver::utils::union_or_single(
                    self.interner,
                    vec![left_true, right_true],
                ));
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_false,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            return Some(right_false);
        }

        None
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
        use crate::query_boundaries::flow_analysis::{
            are_types_mutually_subtype, union_members_for_type,
        };

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
