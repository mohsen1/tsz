//! Assignment type resolution and destructuring matching for `FlowAnalyzer`.
//!
//! Condition-based narrowing (switch clauses, binary/logical expressions, typeof/instanceof)
//! has been extracted to `condition_narrowing.rs`.

use super::{FlowAnalyzer, PropertyKey};
use crate::query_boundaries::flow_analysis::{
    enum_member_domain, evaluate_application_type, fallback_compound_assignment_result,
    get_array_element_type, get_lazy_def_id, is_assignable, is_assignable_with_env,
    is_compound_assignment_operator, map_compound_assignment_to_binary, tuple_elements_for_type,
    union_members_for_type, widen_literal_to_primitive,
};
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{TupleElement, TypeId};

#[derive(Clone, Copy, Debug)]
struct DestructuringSource {
    node: NodeIndex,
    ty: TypeId,
}

impl<'a> FlowAnalyzer<'a> {
    fn node_contains_descendant(&self, ancestor: NodeIndex, mut descendant: NodeIndex) -> bool {
        while descendant.is_some() {
            if descendant == ancestor {
                return true;
            }
            descendant = self
                .arena
                .get_extended(descendant)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
        }
        false
    }

    pub(crate) fn assignment_reads_reference_before_write(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let Some(bin) = self.arena.get_binary_expr(node) else {
                return false;
            };
            if !self.is_assignment_operator(bin.operator_token)
                || !self.assignment_targets_reference_internal(bin.left, reference)
            {
                return false;
            }

            if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                return self.node_contains_descendant(bin.right, reference);
            }

            return self.node_contains_descendant(bin.left, reference)
                || self.node_contains_descendant(bin.right, reference);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(decl) = self.arena.get_variable_declaration(node) else {
                return false;
            };
            return decl.initializer.is_some()
                && self.assignment_targets_reference_internal(decl.name, reference)
                && self.node_contains_descendant(decl.initializer, reference);
        }

        false
    }

    pub(crate) fn is_access_reference(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        })
    }

    fn is_this_access_reference(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(node) else {
            return false;
        };
        self.arena
            .get(access.expression)
            .is_some_and(|base| base.kind == SyntaxKind::ThisKeyword as u16)
    }

    fn is_declared_in_for_in_header(&self, reference: NodeIndex) -> bool {
        let Some(sym_id) = self.reference_symbol(reference) else {
            return false;
        };
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };
        let decl_idx = symbol.value_declaration;
        let Some(decl_info) = self.arena.node_info(decl_idx) else {
            return false;
        };
        let decl_list_idx = decl_info.parent;
        let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
            return false;
        };
        if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        let Some(for_info) = self.arena.node_info(decl_list_idx) else {
            return false;
        };
        let Some(for_node) = self.arena.get(for_info.parent) else {
            return false;
        };
        for_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
    }

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
                if bin.operator_token == SyntaxKind::QuestionQuestionToken as u16
                    || bin.operator_token == SyntaxKind::BarBarToken as u16
                    || bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
                {
                    // Short-circuit expressions like `x ?? (x = y)` or `x || (x = y)`
                    // update the tracked reference when the RHS assignment runs. For the
                    // post-expression flow type, use the whole expression result so the
                    // short-circuit branch and the assignment branch are both reflected.
                    if self.assignment_targets_reference_node(bin.right, target) {
                        if let Some(node_types) = self.node_types
                            && let Some(&expr_type) = node_types.get(&assignment_node.0)
                        {
                            return Some(expr_type);
                        }
                        if let Some(node_types) = self.node_types
                            && let Some(&rhs_type) = node_types.get(&bin.right.0)
                        {
                            return Some(rhs_type);
                        }
                    }
                }
                if bin.operator_token == SyntaxKind::EqualsToken as u16
                    && self.is_declared_in_for_in_header(target)
                {
                    return None;
                }
                // Check if this is a compound assignment operator (not simple =)
                if bin.operator_token != SyntaxKind::EqualsToken as u16
                    && is_compound_assignment_operator(bin.operator_token)
                {
                    if self.is_access_reference(target) {
                        return None;
                    }
                    use crate::query_boundaries::type_computation::core::BinaryOpResult;
                    use tsz_solver::BinaryOpEvaluator;

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
                        // For logical assignments (&&=, ||=, ??=), the post-assignment
                        // type of the LHS must reflect the full expression semantics:
                        //   x ??= y  →  NonNullable<x> | typeof y
                        //   x ||= y  →  Truthy<x> | typeof y
                        //   x &&= y  →  Falsy<x> | typeof y
                        // Use the expression result type (from the entire binary expr
                        // node), not just the RHS type. This ensures that after
                        // `f ??= expr`, the flow type of `f` excludes null/undefined.
                        if let Some(node_types) = self.node_types
                            && let Some(&expr_type) = node_types.get(&assignment_node.0)
                        {
                            return Some(expr_type);
                        }
                        // Fallback to RHS type if expression type not available
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

        if widen_literals_for_destructuring
            && let Some(assigned_type) =
                self.get_destructuring_assigned_type_for_reference(assignment_node, target)
        {
            return Some(assigned_type);
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

            // Also handle `let x; x = []` — assignment of empty array to an
            // unannotated variable declared without initializer. This should
            // also create an evolving array (any[]) matching tsc's behavior.
            let is_rhs_empty_array = self.arena.get(rhs).is_some_and(|rhs_node| {
                rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    && self
                        .arena
                        .get_literal_expr(rhs_node)
                        .is_some_and(|lit| lit.elements.nodes.is_empty())
            });
            if is_rhs_empty_array
                && let Some(sym_id) = self.binder.resolve_identifier(self.arena, target)
                && self.is_control_flow_typed_any_symbol(sym_id)
            {
                return Some(self.interner.array(TypeId::ANY));
            }

            // For flow narrowing, prefer literal types from AST nodes over the type checker's widened types
            // This ensures that `x = 42` narrows to literal 42.0, not just NUMBER
            // This matches TypeScript's behavior where control flow analysis preserves literal types
            if let Some(literal_type) = self.literal_type_from_node(rhs) {
                // For destructuring contexts, return the literal type as-is.
                // narrow_assignment (using one-way assignability) handles the
                // correct narrowing: `[b] = [0]` with `b: 0|1|9` narrows to `0`,
                // while `[c] = [0]` with `c: string|number` narrows to `number`.
                if widen_literals_for_destructuring {
                    return Some(literal_type);
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
                if !is_const {
                    return None;
                }
                if is_structural_literal {
                    if let Some(annotation_type) =
                        self.annotation_type_from_var_decl_node(assignment_node)
                        && let Some(rhs_type) = self
                            .node_types
                            .and_then(|nt| nt.get(&rhs.0).copied())
                            .or_else(|| self.fallback_expression_type_from_syntax(rhs))
                        && self.is_assignable_to(rhs_type, annotation_type)
                    {
                        let reduced = self.narrow_assignment(annotation_type, rhs_type);
                        if reduced != annotation_type {
                            return Some(reduced);
                        }
                    }
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
            let mut cached_rhs_type = None;
            if let Some(node_types) = self.node_types
                && let Some(&rhs_type) = node_types.get(&rhs.0)
            {
                let rhs_type = self
                    .assigned_type_for_await_rhs(rhs, rhs_type)
                    .unwrap_or(rhs_type);
                cached_rhs_type = Some(rhs_type);
                tracing::trace!(
                    "get_assigned_type: node_types HIT for rhs {:?} -> {:?}",
                    rhs,
                    rhs_type
                );
                if rhs_type == TypeId::ERROR {
                    // Loop fixed-point often caches an ERROR placeholder for recursive calls
                    // before the callee has been fully checked. Fall through to the AST-based
                    // fallback instead of short-circuiting on the placeholder.
                } else {
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
                            .and_then(|decl| {
                                // Prefer explicit type annotations when available.
                                // `node_types[decl]` can hold a flow-initialized value type
                                // (e.g. `undefined`) instead of the declared annotation type,
                                // which would incorrectly block assignment-based narrowing.
                                self.annotation_type_from_var_decl_node(decl)
                                    .or_else(|| node_types.get(&decl.0).copied())
                            })
                            .or_else(|| node_types.get(&bin.left.0).copied());

                        if let Some(lhs_type) = declared_target_type
                            && !self.is_assignable_to(rhs_type, lhs_type)
                        {
                            return None;
                        }
                    }
                    return Some(rhs_type);
                }
            }
            let fallback_rhs_type = self.fallback_assigned_type_from_expression(rhs);
            if let Some(rhs_type) = fallback_rhs_type {
                return Some(rhs_type);
            }
            if let Some(rhs_type) = cached_rhs_type {
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
                if self.is_access_reference(target) && !self.is_this_access_reference(target) {
                    return None;
                }
                // When ++/-- is applied to a type that cannot accept number
                // (e.g., unconstrained type parameter T), the operation is
                // invalid (TS2356) and the operand should NOT be narrowed to
                // number. This matches tsc: invalid ++/-- preserves the
                // declared flow type.
                if let Some(node_types) = self.node_types
                    && let Some(&operand_type) = node_types.get(&unary.operand.0)
                    && !self.is_assignable_to(TypeId::NUMBER, operand_type)
                {
                    return None;
                }
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
                return Some(self.for_in_variable_type(expr_type));
            }
            // for-of: extract element type from the array/iterable expression type
            if let Some(elem) = get_array_element_type(self.interner, expr_type) {
                return Some(elem);
            }
        }

        None
    }

    fn get_destructuring_assigned_type_for_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.arena.get(assignment_node)?;
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                    return None;
                }
                let source = self.destructuring_source_from_node(bin.right)?;
                self.match_destructuring_assigned_type(bin.left, source, reference)
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(node)?;
                if decl.initializer.is_none() {
                    return None;
                }
                let source = self.destructuring_source_from_node(decl.initializer)?;
                self.match_destructuring_assigned_type(decl.name, source, reference)
            }
            _ => None,
        }
    }

    fn destructuring_source_from_node(&self, node: NodeIndex) -> Option<DestructuringSource> {
        if node.is_none() {
            return Some(DestructuringSource {
                node: NodeIndex::NONE,
                ty: TypeId::UNDEFINED,
            });
        }

        Some(DestructuringSource {
            node: self.skip_parens_and_assertions(node),
            ty: self.destructuring_source_type_from_node(node)?,
        })
    }

    fn destructuring_source_type_from_node(&self, node: NodeIndex) -> Option<TypeId> {
        if node.is_none() {
            return Some(TypeId::UNDEFINED);
        }

        if let Some(literal_type) = self.literal_type_from_node(node) {
            return Some(literal_type);
        }
        if let Some(nullish_type) = self.nullish_literal_type(node) {
            return Some(nullish_type);
        }

        let stripped = self.skip_parens_and_assertions(node);
        if let Some(tuple_type) = self.destructuring_tuple_type_from_array_literal(node, stripped) {
            return Some(tuple_type);
        }

        if let Some(node_types) = self.node_types {
            if let Some(&ty) = node_types.get(&node.0) {
                return Some(ty);
            }

            if stripped != node {
                return node_types.get(&stripped.0).copied();
            }
        }

        None
    }

    fn match_destructuring_assigned_type(
        &self,
        pattern: NodeIndex,
        source: DestructuringSource,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if pattern.is_none() {
            return None;
        }

        let pattern = self.skip_parens_and_assertions(pattern);

        let node = self.arena.get(pattern)?;

        // Only perform the is_matching_reference short-circuit for simple
        // references (identifiers, property/element access). Compound pattern
        // elements like PROPERTY_ASSIGNMENT or BINDING_ELEMENT wrap an inner
        // reference and need further processing to extract the correct property
        // type from the source. Without this guard, `{ x: b }` would match `b`
        // via reference_symbol transparency and return the entire object source
        // type instead of the property's type.
        if node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT
            && node.kind != syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && node.kind != syntax_kind_ext::BINDING_ELEMENT
            && self.is_matching_reference(pattern, target)
        {
            return Some(source.ty);
        }
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::EqualsToken as u16
                    || !self.assignment_targets_reference_internal(bin.left, target)
                {
                    return None;
                }

                let source = self.destructuring_source_with_default(source, bin.right);
                self.match_destructuring_assigned_type(bin.left, source, target)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                let elements = if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };

                for (index, &elem) in elements.nodes.iter().enumerate() {
                    if elem.is_none() || !self.assignment_targets_reference_internal(elem, target) {
                        continue;
                    }
                    let source = self.destructuring_array_element_source(source, index)?;
                    return self.match_destructuring_assigned_type(elem, source, target);
                }
                None
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
                    if elem.is_none() || !self.assignment_targets_reference_internal(elem, target) {
                        continue;
                    }
                    // Don't extract property here — each element kind (BindingElement,
                    // PropertyAssignment, ShorthandPropertyAssignment) handles its own
                    // property extraction in match_destructuring_assigned_type.
                    return self.match_destructuring_assigned_type(elem, source, target);
                }
                None
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                let binding = self.arena.get_binding_element(node)?;
                if binding.dot_dot_dot_token
                    || !self.assignment_targets_reference_internal(binding.name, target)
                {
                    return None;
                }

                let mut source = source;
                if let Some(ext) = self.arena.get_extended(pattern)
                    && ext.parent.is_some()
                    && let Some(parent_node) = self.arena.get(ext.parent)
                    && parent_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                {
                    let name_idx = if binding.property_name.is_some() {
                        binding.property_name
                    } else {
                        binding.name
                    };
                    source = self.destructuring_property_source(source, name_idx)?;
                }
                if binding.initializer.is_some() {
                    source = self.destructuring_source_with_default(source, binding.initializer);
                }
                self.match_destructuring_assigned_type(binding.name, source, target)
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(node)?;
                if !self.assignment_targets_reference_internal(prop.initializer, target) {
                    return None;
                }

                let source = self.destructuring_property_source(source, prop.name)?;
                self.match_destructuring_assigned_type(prop.initializer, source, target)
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(node)?;
                if !self.assignment_targets_reference_internal(prop.name, target) {
                    return None;
                }

                let mut source = self.destructuring_property_source(source, prop.name)?;
                if prop.object_assignment_initializer.is_some() {
                    source = self.destructuring_source_with_default(
                        source,
                        prop.object_assignment_initializer,
                    );
                }
                self.match_destructuring_assigned_type(prop.name, source, target)
            }
            _ => None,
        }
    }

    fn destructuring_array_element_source(
        &self,
        source: DestructuringSource,
        index: usize,
    ) -> Option<DestructuringSource> {
        let literal_elements = self.array_literal_elements(source.node);
        let node = literal_elements
            .and_then(|elements| elements.nodes.get(index).copied())
            .filter(|node| node.is_some())
            .unwrap_or(NodeIndex::NONE);
        let ty = if literal_elements.is_some() && node.is_none() {
            TypeId::UNDEFINED
        } else if node.is_some() {
            // When the RHS is an array literal and we can identify the specific
            // element node, prefer the element's own type (literal or from
            // node_types) over the array's general element type. This preserves
            // literal narrowing through destructuring: `[b] = [0]` should narrow
            // `b` to literal `0`, not widen to `number`.
            self.destructuring_source_type_from_node(node)
                .unwrap_or_else(|| self.destructuring_numeric_access_type(source.ty, index))
        } else {
            self.destructuring_numeric_access_type(source.ty, index)
        };

        Some(DestructuringSource { node, ty })
    }

    fn destructuring_tuple_type_from_array_literal(
        &self,
        original_node: NodeIndex,
        stripped_node: NodeIndex,
    ) -> Option<TypeId> {
        let stripped_node_data = self.arena.get(stripped_node)?;
        if stripped_node_data.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let typed_source = self
            .node_types
            .and_then(|node_types| node_types.get(&original_node.0).copied())
            .or_else(|| {
                if stripped_node != original_node {
                    self.node_types
                        .and_then(|node_types| node_types.get(&stripped_node.0).copied())
                } else {
                    None
                }
            })?;

        let db = self.interner.as_type_database();
        let is_tuple_like = tuple_elements_for_type(db, typed_source).is_some();
        if !is_tuple_like {
            return None;
        }

        let literal = self.arena.get_literal_expr(stripped_node_data)?;
        let mut tuple_elements = Vec::with_capacity(literal.elements.nodes.len());
        for &element in &literal.elements.nodes {
            if element.is_none() {
                return None;
            }
            let ty = self.destructuring_array_literal_element_type(element)?;
            tuple_elements.push(TupleElement {
                type_id: ty,
                name: None,
                optional: false,
                rest: false,
            });
        }

        Some(self.interner.tuple(tuple_elements))
    }

    fn destructuring_array_literal_element_type(&self, element: NodeIndex) -> Option<TypeId> {
        let element = self.skip_parens_and_assertions(element);
        let element_node = self.arena.get(element)?;
        if element_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(element_node)?;
            if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                return self.destructuring_source_type_from_node(bin.right);
            }
        }

        if element_node.kind == SyntaxKind::Identifier as u16
            && let Some(init_type) = self.destructuring_identifier_initializer_type(element)
        {
            return Some(init_type);
        }

        self.destructuring_source_type_from_node(element)
    }

    fn destructuring_identifier_initializer_type(&self, identifier: NodeIndex) -> Option<TypeId> {
        let symbol = self.binder.resolve_identifier(self.arena, identifier)?;
        let declaration = self.binder.get_symbol(symbol)?.value_declaration;
        let declaration_node = self.arena.get(declaration)?;
        if declaration_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }

        let declaration = self.arena.get_variable_declaration(declaration_node)?;
        if declaration.initializer.is_none() {
            return None;
        }

        self.destructuring_source_type_from_node(declaration.initializer)
    }

    fn destructuring_property_source(
        &self,
        source: DestructuringSource,
        name: NodeIndex,
    ) -> Option<DestructuringSource> {
        let key = self.property_key_from_name(name).or_else(|| {
            if source.node.is_some() {
                self.property_key_from_name_with_rhs_effects(name, source.node)
            } else {
                None
            }
        })?;

        let node = if source.node.is_some() {
            self.lookup_property_in_rhs(source.node, name)
                .unwrap_or(NodeIndex::NONE)
        } else {
            NodeIndex::NONE
        };

        // When the source is a literal (object/array) and we can identify the
        // specific RHS value node, prefer the element's own type over the
        // property-access type from the type system. This preserves literal
        // narrowing: `({ x: b } = { x: 0 })` should narrow `b` to `0`, not
        // widen to `number`.
        let ty = if node.is_some() {
            if let Some(elem_ty) = self.destructuring_source_type_from_node(node) {
                elem_ty
            } else {
                self.destructuring_type_from_key(source.ty, &key)
            }
        } else {
            self.destructuring_type_from_key(source.ty, &key)
        };

        Some(DestructuringSource { node, ty })
    }

    fn destructuring_type_from_key(&self, source_ty: TypeId, key: &PropertyKey) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;
        match key {
            PropertyKey::Index(index) => self.destructuring_numeric_access_type(source_ty, *index),
            PropertyKey::Atom(atom) => match self.interner.resolve_property_access_with_options(
                source_ty,
                &self.interner.resolve_atom_ref(*atom),
                self.interner.no_unchecked_indexed_access(),
            ) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                PropertyAccessResult::PropertyNotFound { .. } => TypeId::UNDEFINED,
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    property_type.unwrap_or(TypeId::UNDEFINED)
                }
                PropertyAccessResult::IsUnknown => TypeId::UNKNOWN,
            },
        }
    }

    fn destructuring_numeric_access_type(&self, source_ty: TypeId, index: usize) -> TypeId {
        let mut visited = FxHashSet::default();
        self.destructuring_numeric_access_type_inner(source_ty, index, &mut visited)
    }

    fn destructuring_numeric_access_type_inner(
        &self,
        source_ty: TypeId,
        index: usize,
        visited: &mut FxHashSet<TypeId>,
    ) -> TypeId {
        if !visited.insert(source_ty) {
            return TypeId::UNKNOWN;
        }

        let db = self.interner.as_type_database();
        let result = if let Some(members) = union_members_for_type(db, source_ty) {
            let mut member_types = Vec::new();
            for member in members {
                let member_type =
                    self.destructuring_numeric_access_type_inner(member, index, visited);
                if member_type != TypeId::NEVER {
                    member_types.push(member_type);
                }
            }
            tsz_solver::utils::union_or_single(self.interner, member_types)
        } else if let Some(elements) = tuple_elements_for_type(db, source_ty) {
            let mut result = TypeId::UNDEFINED;
            for (position, element) in elements.iter().enumerate() {
                if element.rest {
                    if index >= position {
                        let elem_ty =
                            get_array_element_type(db, element.type_id).unwrap_or(element.type_id);
                        // With noUncheckedIndexedAccess, rest-region elements
                        // are potentially undefined (the tuple length is unknown).
                        result = if self.interner.no_unchecked_indexed_access() {
                            self.interner.union2(elem_ty, TypeId::UNDEFINED)
                        } else {
                            elem_ty
                        };
                    }
                    break;
                }

                if index == position {
                    result = if element.optional {
                        self.interner.union2(element.type_id, TypeId::UNDEFINED)
                    } else {
                        element.type_id
                    };
                    break;
                }
            }
            result
        } else if let Some(element_type) = get_array_element_type(db, source_ty) {
            if self.interner.no_unchecked_indexed_access() {
                self.interner.union2(element_type, TypeId::UNDEFINED)
            } else {
                element_type
            }
        } else {
            TypeId::UNKNOWN
        };

        visited.remove(&source_ty);
        result
    }

    fn destructuring_source_with_default(
        &self,
        source: DestructuringSource,
        default_node: NodeIndex,
    ) -> DestructuringSource {
        let source_node = self.skip_parens_and_assertions(default_node);
        let Some(default_ty) = self.destructuring_source_type_from_node(default_node) else {
            return source;
        };

        let non_undefined = crate::query_boundaries::flow::narrow_destructuring_default(
            self.interner.as_type_database(),
            source.ty,
            true,
        );
        let ty = if non_undefined == TypeId::NEVER {
            default_ty
        } else {
            self.interner.union2(non_undefined, default_ty)
        };

        let node = source_node;

        DestructuringSource { node, ty }
    }

    pub(crate) fn is_await_assignment_for_reference(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        self.assignment_rhs_for_reference(assignment_node, target)
            .and_then(|rhs| self.arena.get(rhs))
            .is_some_and(|rhs_node| rhs_node.kind == syntax_kind_ext::AWAIT_EXPRESSION)
    }

    /// Compute the type of a for-in loop variable.
    ///
    /// Matches tsc: when the expression type has a type parameter, the variable
    /// gets `keyof T & string` (= `Extract<keyof T, string>`) so that `obj[k]`
    /// is well-typed. Otherwise returns plain `string`.
    fn for_in_variable_type(&self, expr_type: TypeId) -> TypeId {
        use crate::query_boundaries::flow_analysis as query;

        let db = self.interner.as_type_database();
        let non_nullable =
            crate::query_boundaries::flow::remove_nullish_for_iteration(db, expr_type);

        // For concrete (non-generic) types, always return `string`.
        // Computing `keyof ConcreteType` creates a KeyOf node that may not
        // fully evaluate, causing `is_keyof_type` to return true and leak
        // `keyof T & string` into the variable's flow type. This prevents
        // TS7053 from firing when indexing with for-in variables.
        if !query::contains_type_parameters(db, non_nullable) {
            return TypeId::STRING;
        }

        let keyof_type = self.interner.factory().keyof(non_nullable);

        if query::is_type_parameter_like(db, keyof_type) || query::is_keyof_type(db, keyof_type) {
            self.interner
                .factory()
                .intersection2(keyof_type, TypeId::STRING)
        } else {
            TypeId::STRING
        }
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

        let initial_type = self.resolve_assignment_reduction_type(initial_type);

        // For enum types, narrow directly to the assigned type when it is
        // assignable to the enum. This preserves the enum member identity
        // (e.g. `E.ONE` stays `E.ONE`, not decomposed to `0`).
        // E.g. `let e: E = E.ONE` narrows the flow type from `E` to `E.ONE`.
        if enum_member_domain(self.interner, initial_type) != initial_type {
            let assigned_type = self.resolve_assignment_reduction_type(assigned_type);
            return if is_assignable(self.interner, assigned_type, initial_type) {
                assigned_type
            } else {
                initial_type
            };
        }

        let members_opt = union_members_for_type(self.interner, initial_type);
        let members = match members_opt {
            Some(m) => m,
            None => return initial_type,
        };

        if members.len() <= 1 {
            return initial_type;
        }

        // Resolve Lazy(DefId) types to their concrete representations via the
        // TypeEnvironment before structural subtype comparison. node_types may
        // store unevaluated type alias references when a variable was inferred
        // (no annotation), while union members are already resolved. Without this,
        // the bare SubtypeChecker used by are_types_mutually_subtype cannot match
        // a Lazy(DefId) against a concrete Object type, causing narrowing to fail.
        let assigned_type = self.resolve_assignment_reduction_type(assigned_type);

        // Match tsc's getAssignmentReducedType: keep union members where the
        // assigned type is assignable to the member. This is one-way
        // assignability (`typeMaybeAssignableTo(assignedType, member)` in tsc),
        // NOT mutual subtype. One-way is essential for cases like:
        //   - `let b: 0|1|9; [b] = [0];` → b narrows to 0 (0 <: 0)
        //   - `let c: string|number; c = 0;` → c narrows to number (0 <: number)
        let mut kept = Vec::new();
        for &m in &members {
            let assignable_to_member = if let Some(env) = &self.type_environment {
                is_assignable_with_env(self.interner, &env.borrow(), assigned_type, m, true)
            } else {
                is_assignable(self.interner, assigned_type, m)
            };
            if assignable_to_member {
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

    /// Resolve a `Lazy(DefId)` type to its concrete representation using the
    /// `TypeEnvironment`. Returns the original type if not lazy or if the
    /// environment is unavailable / doesn't contain the DefId.
    pub(super) fn resolve_lazy_via_env(&self, type_id: TypeId) -> TypeId {
        if let Some(def_id) = get_lazy_def_id(self.interner, type_id) {
            if let Some(ref env) = self.type_environment {
                env.borrow().get_def(def_id).unwrap_or(type_id)
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    fn resolve_assignment_reduction_type(&self, type_id: TypeId) -> TypeId {
        let resolved = self.resolve_lazy_via_env(type_id);
        let Some(env) = &self.type_environment else {
            return resolved;
        };
        let env = env.borrow();
        evaluate_application_type(self.interner, &env, resolved)
    }
}
