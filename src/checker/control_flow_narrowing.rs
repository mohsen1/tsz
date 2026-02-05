//! Control Flow Narrowing (continued)
//!
//! Extracted from control_flow.rs: Second half of FlowAnalyzer impl block
//! containing narrowing methods for assignments, predicates, instanceof,
//! in-operator, typeof, discriminants, literals, and reference matching.

use crate::binder::{SymbolId, symbol_flags};
use crate::interner::Atom;
use crate::parser::node::CallExprData;
use crate::parser::{NodeIndex, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::solver::{
    LiteralValue, NarrowingContext, ParamInfo, TypeGuard, TypeId, TypePredicate,
    TypePredicateTarget,
    type_queries::{
        ConstructorInstanceKind, FalsyComponentKind, LiteralValueKind, NonObjectKind,
        PredicateSignatureKind, PropertyPresenceKind, TypeParameterConstraintKind,
        UnionMembersKind, classify_for_constructor_instance, classify_for_falsy_component,
        classify_for_literal_value, classify_for_non_object, classify_for_predicate_signature,
        classify_for_property_presence, classify_for_type_parameter_constraint,
        classify_for_union_members,
    },
};
use std::borrow::Cow;

use super::control_flow::{FlowAnalyzer, PredicateSignature, PropertyKey, PropertyPresence};

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
        if let Some(callee_node) = self.arena.get(call.expression) {
            if (callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.arena.get_access_expr(callee_node)
                && access.question_dot_token
            {
                // For optional chaining, only narrow the true branch
                if !is_true_branch {
                    return None;
                }
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
                // TODO: Handle spread arguments correctly.
                // Currently assumes 1:1 mapping which breaks for `fn(...args)`.
                let args = call.arguments.as_ref()?.nodes.as_slice();
                args.get(param_index).copied()
            }
            TypePredicateTarget::This => {
                // CRITICAL: Skip parens/assertions to find the actual access node
                // Handles cases like (obj.isString)() and (obj.isString as any)()
                let callee_idx = self.skip_parens_and_assertions(call.expression);
                let callee_node = self.arena.get(callee_idx)?;

                // Check for PropertyAccess or ElementAccess
                if let Some(access) = self.arena.get_access_expr(callee_node) {
                    return Some(access.expression);
                }

                None
            }
        }
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

        let narrowing = NarrowingContext::new(self.interner);

        if let Some(predicate_type) = predicate.type_id {
            if is_true_branch {
                return narrowing.narrow_to_type(type_id, predicate_type);
            }
            return narrowing.narrow_excluding_type(type_id, predicate_type);
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
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        if !is_true_branch {
            return type_id;
        }

        if !self.is_matching_reference(bin.left, target) {
            return type_id;
        }

        // Special case for unknown: instanceof narrows to object type
        // This handles cases like: if (error instanceof Error) where error: unknown
        if type_id == TypeId::UNKNOWN {
            if let Some(instance_type) = self.instance_type_from_constructor(bin.right) {
                return instance_type;
            }
            return TypeId::OBJECT;
        }

        if let Some(instance_type) = self.instance_type_from_constructor(bin.right) {
            let narrowing = NarrowingContext::new(self.interner);
            return narrowing.narrow_to_type(type_id, instance_type);
        }

        self.narrow_to_objectish(type_id)
    }

    pub(crate) fn instance_type_from_constructor(&self, expr: NodeIndex) -> Option<TypeId> {
        if let Some(node_types) = self.node_types
            && let Some(&type_id) = node_types.get(&expr.0)
            && let Some(instance_type) = self.instance_type_from_constructor_type(type_id)
        {
            return Some(instance_type);
        }

        let expr = self.skip_parens_and_assertions(expr);
        let sym_id = self.binder.resolve_identifier(self.arena, expr)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::CLASS) != 0 {
            return Some(self.interner.reference(crate::solver::SymbolRef(sym_id.0)));
        }

        None
    }

    pub(crate) fn instance_type_from_constructor_type(&self, type_id: TypeId) -> Option<TypeId> {
        match classify_for_constructor_instance(self.interner, type_id) {
            ConstructorInstanceKind::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                if shape.construct_signatures.is_empty() {
                    return None;
                }
                let mut returns = Vec::new();
                for sig in &shape.construct_signatures {
                    returns.push(sig.return_type);
                }
                Some(if returns.len() == 1 {
                    returns[0]
                } else {
                    self.interner.union(returns)
                })
            }
            ConstructorInstanceKind::Union(members) => {
                let mut instance_types = Vec::new();
                for member in members {
                    if let Some(instance_type) = self.instance_type_from_constructor_type(member) {
                        instance_types.push(instance_type);
                    }
                }
                if instance_types.is_empty() {
                    None
                } else if instance_types.len() == 1 {
                    Some(instance_types[0])
                } else {
                    Some(self.interner.union(instance_types))
                }
            }
            ConstructorInstanceKind::None => None,
        }
    }

    pub(crate) fn narrow_by_in_operator(
        &self,
        type_id: TypeId,
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        if !self.is_matching_reference(bin.right, target) {
            return type_id;
        }

        let Some((prop_name, prop_is_number)) = self.in_property_name(bin.left) else {
            return type_id;
        };

        if type_id == TypeId::ANY {
            return type_id;
        }

        // For UNKNOWN, 'prop' in x narrows to object types
        // TypeScript allows narrowing unknown through 'in' operator
        if type_id == TypeId::UNKNOWN {
            // 'prop' in x narrows unknown to object types (objects, arrays, etc.)
            // Primitives don't have properties that can be checked with 'in'
            return TypeId::OBJECT;
        }

        if let TypeParameterConstraintKind::TypeParameter { constraint } =
            classify_for_type_parameter_constraint(self.interner, type_id)
        {
            if let Some(constraint) = constraint {
                if constraint != type_id {
                    let narrowed_constraint =
                        self.narrow_by_in_operator(constraint, bin, target, is_true_branch);
                    if narrowed_constraint != constraint {
                        return self.interner.intersection2(type_id, narrowed_constraint);
                    }
                }
            }
            return type_id;
        }

        let UnionMembersKind::Union(members) = classify_for_union_members(self.interner, type_id)
        else {
            return type_id;
        };

        let members_len = members.len();
        let mut filtered = Vec::new();
        for member in members {
            let presence = self.property_presence(member, prop_name, prop_is_number);
            if self.keep_in_operator_member(presence, is_true_branch) {
                filtered.push(member);
            }
        }

        match filtered.len() {
            0 => TypeId::NEVER,
            1 => filtered[0],
            _ => {
                if filtered.len() == members_len {
                    type_id
                } else {
                    self.interner.union(filtered)
                }
            }
        }
    }

    pub(crate) fn narrow_to_objectish(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY {
            return type_id;
        }
        // For UNKNOWN, typeof x === "object" narrows to non-primitive types
        // TypeScript allows narrowing unknown through typeof checks
        if type_id == TypeId::UNKNOWN {
            // typeof x === "object" narrows unknown to object types (excluding primitives)
            // This is a union of object, array, tuple, function, etc.
            return TypeId::OBJECT;
        }

        if let UnionMembersKind::Union(members) = classify_for_union_members(self.interner, type_id)
        {
            let members_len = members.len();
            let mut kept = Vec::new();
            for member in members {
                if !self.is_definitely_non_object(member) {
                    kept.push(member);
                }
            }

            return match kept.len() {
                0 => TypeId::NEVER,
                1 => kept[0],
                _ => {
                    if kept.len() == members_len {
                        type_id
                    } else {
                        self.interner.union(kept)
                    }
                }
            };
        }

        if self.is_definitely_non_object(type_id) {
            TypeId::NEVER
        } else {
            type_id
        }
    }

    pub(crate) fn is_definitely_non_object(&self, type_id: TypeId) -> bool {
        if matches!(
            type_id,
            TypeId::NEVER
                | TypeId::VOID
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::BOOLEAN
                | TypeId::NUMBER
                | TypeId::STRING
                | TypeId::BIGINT
                | TypeId::SYMBOL
        ) {
            return true;
        }

        match classify_for_non_object(self.interner, type_id) {
            NonObjectKind::Literal | NonObjectKind::IntrinsicPrimitive => true,
            NonObjectKind::MaybeObject => false,
        }
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

    pub(crate) fn keep_in_operator_member(
        &self,
        presence: PropertyPresence,
        is_true_branch: bool,
    ) -> bool {
        match (presence, is_true_branch) {
            (PropertyPresence::Required, false) => false,
            (PropertyPresence::Absent, true) => false,
            _ => true,
        }
    }

    pub(crate) fn property_presence(
        &self,
        type_id: TypeId,
        prop_name: Atom,
        prop_is_number: bool,
    ) -> PropertyPresence {
        match classify_for_property_presence(self.interner, type_id) {
            PropertyPresenceKind::IntrinsicObject => PropertyPresence::Unknown,
            PropertyPresenceKind::Object(shape_id) => {
                self.property_presence_in_object(shape_id, prop_name, prop_is_number)
            }
            PropertyPresenceKind::Callable(callable_id) => {
                self.property_presence_in_callable(callable_id, prop_name)
            }
            PropertyPresenceKind::ArrayLike => {
                if prop_is_number {
                    PropertyPresence::Optional
                } else {
                    PropertyPresence::Unknown
                }
            }
            PropertyPresenceKind::Unknown => PropertyPresence::Unknown,
        }
    }

    pub(crate) fn property_presence_in_object(
        &self,
        shape_id: crate::solver::ObjectShapeId,
        prop_name: Atom,
        prop_is_number: bool,
    ) -> PropertyPresence {
        let shape = self.interner.object_shape(shape_id);
        let mut found = None;

        match self.interner.object_property_index(shape_id, prop_name) {
            crate::solver::PropertyLookup::Found(idx) => {
                found = shape.properties.get(idx);
            }
            crate::solver::PropertyLookup::Uncached => {
                found = shape.properties.iter().find(|prop| prop.name == prop_name);
            }
            crate::solver::PropertyLookup::NotFound => {}
        }

        if let Some(prop) = found {
            return if prop.optional {
                PropertyPresence::Optional
            } else {
                PropertyPresence::Required
            };
        }

        if prop_is_number && shape.number_index.is_some() {
            return PropertyPresence::Optional;
        }

        if shape.string_index.is_some() {
            return PropertyPresence::Optional;
        }

        PropertyPresence::Absent
    }

    pub(crate) fn property_presence_in_callable(
        &self,
        callable_id: crate::solver::CallableShapeId,
        prop_name: Atom,
    ) -> PropertyPresence {
        let shape = self.interner.callable_shape(callable_id);
        if let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_name) {
            return if prop.optional {
                PropertyPresence::Optional
            } else {
                PropertyPresence::Required
            };
        }
        PropertyPresence::Absent
    }

    pub(crate) fn union_types(&self, left: TypeId, right: TypeId) -> TypeId {
        if left == right {
            left
        } else {
            self.interner.union(vec![left, right])
        }
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
            _ => None,
        }
    }

    pub(crate) fn literal_assignable_to(
        &self,
        literal: TypeId,
        target: TypeId,
        narrowing: &NarrowingContext,
    ) -> bool {
        if literal == target || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        if let UnionMembersKind::Union(members) = classify_for_union_members(self.interner, target)
        {
            return members
                .iter()
                .any(|&member| self.literal_assignable_to(literal, member, narrowing));
        }

        narrowing.narrow_to_type(literal, target) != TypeId::NEVER
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
        let (_full_path, _is_optional, _base, relative_info) =
            self.discriminant_property_info(expr, target)?;

        // Prefer relative path if available (nested narrowing)
        if let Some((rel_path, is_optional, _)) = relative_info {
            if is_optional || rel_path.is_empty() {
                return None;
            }
            return Some(rel_path);
        }

        // Fallback to base narrowing
        self.discriminant_property_info(expr, target).and_then(
            |(path, is_optional, base, _relative)| {
                if is_optional || !self.is_matching_reference(base, target) {
                    None
                } else {
                    Some(path)
                }
            },
        )
    }

    pub(crate) fn discriminant_property_info(
        &self,
        expr: NodeIndex,
        target: NodeIndex,
    ) -> Option<(
        Vec<Atom>,
        bool,
        NodeIndex,
        Option<(Vec<Atom>, bool, NodeIndex)>,
    )> {
        let expr = self.skip_parenthesized(expr);
        let _node = self.arena.get(expr)?;

        // Collect the property path by walking up the access chain
        // For action.payload.kind, we want ["payload", "kind"]
        let mut path: Vec<Atom> = Vec::new();
        let mut is_optional = false;
        let mut current = expr;

        // Track relative path if target matches an intermediate node
        let mut relative_path_info: Option<(Vec<Atom>, bool, NodeIndex)> = None;

        loop {
            // CRITICAL: Check if CURRENT matches target BEFORE adding the next segment
            // This ensures the relative path is from the target to the leaf
            if self.is_matching_reference(current, target) {
                // Found the target at an intermediate level
                // Clone the current path for the relative path (from target to leaf)
                let mut relative = path.clone();
                relative.reverse();
                relative_path_info = Some((relative, is_optional, current));
            }

            let current_node = self.arena.get(current)?;
            let access = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.arena.get_access_expr(current_node)?
            } else if current_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
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
                let name_node = self.arena.get(access.name_or_argument)?;
                let ident = self.arena.get_identifier(name_node)?;
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

            // Unwrap assignment expressions to get the actual target
            let effective_target = if access_target_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            {
                let binary = self.arena.get_binary_expr(access_target_node)?;
                if binary.operator_token == SyntaxKind::EqualsToken as u16 {
                    binary.left
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
        Some((path, is_optional, current, relative_path_info))
    }

    pub(crate) fn discriminant_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, TypeId, bool, NodeIndex)> {
        eprintln!(
            "DEBUG discriminant_comparison: left={}, right={}, target={}",
            left.0, right.0, target.0
        );

        if let Some((full_path, is_optional, base, relative_info)) =
            self.discriminant_property_info(left, target)
            && let Some(literal) = self.literal_type_from_node(right)
        {
            eprintln!(
                "DEBUG discriminant_comparison: full_path={:?}, base={}, relative_info={:?}",
                full_path,
                base.0,
                relative_info.is_some()
            );

            // Check if target matches an intermediate node (nested narrowing)
            if let Some((rel_path, rel_optional, rel_base)) = relative_info {
                // CRITICAL: If rel_path is empty, target is the leaf node
                // In this case, we should use literal comparison, not discriminant narrowing
                if !rel_path.is_empty() {
                    eprintln!(
                        "DEBUG discriminant_comparison: MATCHED relative - rel_path={:?}, rel_base={}",
                        rel_path, rel_base.0
                    );
                    // For action.payload.kind === 'item' where target is action.payload:
                    // - rel_path = ['kind']
                    // - rel_base = action.payload
                    // - This narrows action.payload based on the 'kind' property
                    return Some((rel_path, literal, rel_optional, rel_base));
                }
                eprintln!(
                    "DEBUG discriminant_comparison: rel_path is empty (target is leaf), falling through"
                );
            }

            // Check if target matches the base (root narrowing)
            if self.is_matching_reference(base, target) {
                eprintln!("DEBUG discriminant_comparison: MATCHED base");
                // For action.payload.kind === 'item' where target is action:
                // - full_path = ['payload', 'kind']
                // - base = action
                // This narrows action based on the full path
                return Some((full_path, literal, is_optional, base));
            }
        }

        if let Some((full_path, is_optional, base, relative_info)) =
            self.discriminant_property_info(right, target)
            && let Some(literal) = self.literal_type_from_node(left)
        {
            eprintln!(
                "DEBUG discriminant_comparison (swapped): full_path={:?}, base={}, relative_info={:?}",
                full_path,
                base.0,
                relative_info.is_some()
            );

            // Check if target matches an intermediate node (nested narrowing)
            if let Some((rel_path, rel_optional, rel_base)) = relative_info {
                // CRITICAL: If rel_path is empty, target is the leaf node
                if !rel_path.is_empty() {
                    eprintln!(
                        "DEBUG discriminant_comparison (swapped): MATCHED relative - rel_path={:?}, rel_base={}",
                        rel_path, rel_base.0
                    );
                    return Some((rel_path, literal, rel_optional, rel_base));
                }
            }

            // Check if target matches the base (root narrowing)
            if self.is_matching_reference(base, target) {
                eprintln!("DEBUG discriminant_comparison (swapped): MATCHED base");
                return Some((full_path, literal, is_optional, base));
            }
        }

        eprintln!("DEBUG discriminant_comparison: NO MATCH");
        None
    }

    pub(crate) fn narrow_by_discriminant_for_type(
        &self,
        type_id: TypeId,
        prop_path: &[Atom],
        literal_type: TypeId,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        if let TypeParameterConstraintKind::TypeParameter {
            constraint: Some(constraint),
        } = classify_for_type_parameter_constraint(self.interner, type_id)
        {
            if constraint != type_id {
                let narrowed_constraint = if is_true_branch {
                    narrowing.narrow_by_discriminant(constraint, prop_path, literal_type)
                } else {
                    narrowing.narrow_by_excluding_discriminant(constraint, prop_path, literal_type)
                };
                if narrowed_constraint != constraint {
                    return self.interner.intersection2(type_id, narrowed_constraint);
                }
            }
        }

        if is_true_branch {
            narrowing.narrow_by_discriminant(type_id, prop_path, literal_type)
        } else {
            narrowing.narrow_by_excluding_discriminant(type_id, prop_path, literal_type)
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

    pub(crate) fn narrow_by_typeof_negation(
        &self,
        type_id: TypeId,
        typeof_result: &str,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        match typeof_result {
            "string" => narrowing.narrow_excluding_type(type_id, TypeId::STRING),
            "number" => narrowing.narrow_excluding_type(type_id, TypeId::NUMBER),
            "boolean" => narrowing.narrow_excluding_type(type_id, TypeId::BOOLEAN),
            "bigint" => narrowing.narrow_excluding_type(type_id, TypeId::BIGINT),
            "symbol" => narrowing.narrow_excluding_type(type_id, TypeId::SYMBOL),
            "undefined" => narrowing.narrow_excluding_type(type_id, TypeId::UNDEFINED),
            "object" => narrowing.narrow_excluding_type(type_id, TypeId::OBJECT),
            "function" => narrowing.narrow_excluding_function(type_id),
            _ => type_id,
        }
    }

    pub(crate) fn narrow_to_falsy(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY {
            return type_id;
        }
        // For UNKNOWN, we can narrow to the union of all falsy types
        // TypeScript allows narrowing unknown through type guards
        if type_id == TypeId::UNKNOWN {
            return self.interner.union(vec![
                TypeId::NULL,
                TypeId::UNDEFINED,
                self.interner.literal_boolean(false),
                self.interner.literal_string(""),
                self.interner.literal_number(0.0),
                self.interner.literal_bigint("0"),
            ]);
        }

        match self.falsy_component(type_id) {
            Some(falsy) => falsy,
            None => TypeId::NEVER,
        }
    }

    pub(crate) fn falsy_component(&self, type_id: TypeId) -> Option<TypeId> {
        if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
            return Some(type_id);
        }
        if type_id == TypeId::BOOLEAN {
            return Some(self.interner.literal_boolean(false));
        }
        if type_id == TypeId::STRING {
            return Some(self.interner.literal_string(""));
        }
        if type_id == TypeId::NUMBER {
            return Some(self.interner.literal_number(0.0));
        }
        if type_id == TypeId::BIGINT {
            return Some(self.interner.literal_bigint("0"));
        }

        match classify_for_falsy_component(self.interner, type_id) {
            FalsyComponentKind::Literal(literal) => {
                if self.literal_is_falsy(&literal) {
                    Some(type_id)
                } else {
                    None
                }
            }
            FalsyComponentKind::Union(members) => {
                let mut falsy_members = Vec::new();
                for member in members {
                    if let Some(falsy) = self.falsy_component(member) {
                        falsy_members.push(falsy);
                    }
                }
                match falsy_members.len() {
                    0 => None,
                    1 => Some(falsy_members[0]),
                    _ => Some(self.interner.union(falsy_members)),
                }
            }
            FalsyComponentKind::TypeParameter => Some(type_id),
            FalsyComponentKind::None => None,
        }
    }

    pub(crate) fn literal_is_falsy(&self, literal: &LiteralValue) -> bool {
        match literal {
            LiteralValue::Boolean(false) => true,
            LiteralValue::Number(value) => value.0 == 0.0,
            LiteralValue::String(atom) => self.interner.resolve_atom(*atom).is_empty(),
            LiteralValue::BigInt(atom) => self.interner.resolve_atom(*atom) == "0",
            _ => false,
        }
    }

    pub(crate) fn strip_numeric_separators<'b>(&self, text: &'b str) -> Cow<'b, str> {
        if !text.as_bytes().contains(&b'_') {
            return Cow::Borrowed(text);
        }

        let mut out = String::with_capacity(text.len());
        for &byte in text.as_bytes() {
            if byte != b'_' {
                out.push(byte as char);
            }
        }
        Cow::Owned(out)
    }

    pub(crate) fn parse_numeric_literal_value(
        &self,
        value: Option<f64>,
        text: &str,
    ) -> Option<f64> {
        if let Some(value) = value {
            return Some(value);
        }

        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::parse_radix_digits(rest, 16);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::parse_radix_digits(rest, 2);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::parse_radix_digits(rest, 8);
        }

        if text.as_bytes().contains(&b'_') {
            let cleaned = self.strip_numeric_separators(text);
            return cleaned.as_ref().parse::<f64>().ok();
        }

        text.parse::<f64>().ok()
    }

    pub(crate) fn parse_radix_digits(text: &str, base: u32) -> Option<f64> {
        if text.is_empty() {
            return None;
        }

        let mut value = 0f64;
        let base_value = base as f64;
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;
            value = value * base_value + digit as f64;
        }

        if !saw_digit {
            return None;
        }

        Some(value)
    }

    pub(crate) fn normalize_bigint_literal<'b>(&self, text: &'b str) -> Option<Cow<'b, str>> {
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::bigint_base_to_decimal(rest, 16).map(Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::bigint_base_to_decimal(rest, 2).map(Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::bigint_base_to_decimal(rest, 8).map(Cow::Owned);
        }

        match self.strip_numeric_separators(text) {
            Cow::Borrowed(cleaned) => {
                let trimmed = cleaned.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned.len() {
                    return Some(Cow::Borrowed(cleaned));
                }
                Some(Cow::Borrowed(trimmed))
            }
            Cow::Owned(mut cleaned) => {
                let cleaned_ref = cleaned.as_str();
                let trimmed = cleaned_ref.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned_ref.len() {
                    return Some(Cow::Owned(cleaned));
                }

                let trim_len = cleaned_ref.len() - trimmed.len();
                cleaned.drain(..trim_len);
                Some(Cow::Owned(cleaned))
            }
        }
    }

    pub(crate) fn bigint_base_to_decimal(text: &str, base: u32) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        let mut digits: Vec<u8> = vec![0];
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;

            let mut carry = digit;
            for slot in &mut digits {
                let value = (*slot as u32) * base + carry;
                *slot = (value % 10) as u8;
                carry = value / 10;
            }
            while carry > 0 {
                digits.push((carry % 10) as u8);
                carry /= 10;
            }
        }

        if !saw_digit {
            return None;
        }

        while digits.len() > 1 {
            if let Some(&last) = digits.last() {
                if last == 0 {
                    digits.pop();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let mut out = String::with_capacity(digits.len());
        for digit in digits.iter().rev() {
            out.push(char::from(b'0' + *digit));
        }
        Some(out)
    }

    /// Check if two references point to the same symbol or property access chain.
    pub(crate) fn is_matching_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let a = self.skip_parenthesized(a);
        let b = self.skip_parenthesized(b);

        if let (Some(node_a), Some(node_b)) = (self.arena.get(a), self.arena.get(b)) {
            if node_a.kind == SyntaxKind::ThisKeyword as u16
                && node_b.kind == SyntaxKind::ThisKeyword as u16
            {
                return true;
            }
            if node_a.kind == SyntaxKind::SuperKeyword as u16
                && node_b.kind == SyntaxKind::SuperKeyword as u16
            {
                return true;
            }
        }

        let sym_a = self.reference_symbol(a);
        let sym_b = self.reference_symbol(b);
        if sym_a.is_some() && sym_a == sym_b {
            return true;
        }

        self.is_matching_property_reference(a, b)
    }

    pub(crate) fn is_matching_property_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let Some((a_base, a_name)) = self.property_reference(a) else {
            return false;
        };
        let Some((b_base, b_name)) = self.property_reference(b) else {
            return false;
        };
        if a_name != b_name {
            return false;
        }
        self.is_matching_reference(a_base, b_base)
    }

    pub(crate) fn property_reference(&self, idx: NodeIndex) -> Option<(NodeIndex, Atom)> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.property_reference(unary.expression);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.property_reference(assertion.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name_node = self.arena.get(access.name_or_argument)?;
            let ident = self.arena.get_identifier(name_node)?;
            let name = self.interner.intern_string(&ident.escaped_text);
            return Some((access.expression, name));
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name = self.literal_atom_from_node_or_type(access.name_or_argument)?;
            return Some((access.expression, name));
        }

        None
    }

    pub(crate) fn literal_atom_from_node_or_type(&self, idx: NodeIndex) -> Option<Atom> {
        if let Some(name) = self.literal_string_from_node(idx) {
            return Some(self.interner.intern_string(name));
        }
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some(self.atom_from_numeric_value(value));
        }
        self.literal_atom_from_type(idx)
    }

    pub(crate) fn literal_atom_and_kind_from_node_or_type(
        &self,
        idx: NodeIndex,
    ) -> Option<(Atom, bool)> {
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some((self.atom_from_numeric_value(value), true));
        }
        if let Some(name) = self.literal_string_from_node(idx) {
            return Some((self.interner.intern_string(name), false));
        }

        // Handle private identifiers (e.g., #a in x)
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            let ident = self.arena.get_identifier(node)?;
            return Some((self.interner.intern_string(&ident.escaped_text), false));
        }

        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match classify_for_literal_value(self.interner, type_id) {
            LiteralValueKind::String(atom) => Some((atom, false)),
            LiteralValueKind::Number(value) => Some((self.atom_from_numeric_value(value), true)),
            LiteralValueKind::None => None,
        }
    }

    pub(crate) fn literal_number_from_node_or_type(&self, idx: NodeIndex) -> Option<f64> {
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some(value);
        }
        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match classify_for_literal_value(self.interner, type_id) {
            LiteralValueKind::Number(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn literal_atom_from_type(&self, idx: NodeIndex) -> Option<Atom> {
        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match classify_for_literal_value(self.interner, type_id) {
            LiteralValueKind::String(atom) => Some(atom),
            LiteralValueKind::Number(value) => Some(self.atom_from_numeric_value(value)),
            LiteralValueKind::None => None,
        }
    }

    pub(crate) fn property_key_from_name(&self, name_idx: NodeIndex) -> Option<PropertyKey> {
        let name_idx = self.skip_parens_and_assertions(name_idx);
        let node = self.arena.get(name_idx)?;

        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            if let Some(value) = self.literal_number_from_node_or_type(computed.expression)
                && value.fract() == 0.0
                && value >= 0.0
            {
                return Some(PropertyKey::Index(value as usize));
            }
            if let Some(atom) = self.literal_atom_from_node_or_type(computed.expression) {
                return Some(PropertyKey::Atom(atom));
            }
            return None;
        }

        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(PropertyKey::Atom(
                self.interner.intern_string(&ident.escaped_text),
            ));
        }

        if let Some((atom, _)) = self.literal_atom_and_kind_from_node_or_type(name_idx) {
            return Some(PropertyKey::Atom(atom));
        }

        None
    }

    pub(crate) fn literal_number_from_node(&self, idx: NodeIndex) -> Option<f64> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                self.parse_numeric_literal_value(lit.value, &lit.text)
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = self.skip_parenthesized(unary.operand);
                let operand_node = self.arena.get(operand)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.arena.get_literal(operand_node)?;
                let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                Some(if op == SyntaxKind::MinusToken as u16 {
                    -value
                } else {
                    value
                })
            }
            _ => None,
        }
    }

    pub(crate) fn atom_from_numeric_value(&self, value: f64) -> Atom {
        let name = if value == 0.0 && value.is_sign_negative() {
            "-0".to_string()
        } else if value.fract() == 0.0 {
            format!("{:.0}", value)
        } else {
            format!("{}", value)
        };
        self.interner.intern_string(&name)
    }

    pub(crate) fn reference_base(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.reference_base(unary.expression);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.reference_base(assertion.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            return Some(access.expression);
        }

        None
    }

    pub(crate) fn reference_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let mut visited = Vec::new();
        self.reference_symbol_inner(idx, &mut visited)
    }

    pub(crate) fn reference_symbol_inner(
        &self,
        idx: NodeIndex,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let idx = self.skip_parenthesized(idx);
        if let Some(sym_id) = self
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.binder.resolve_identifier(self.arena, idx))
        {
            return self.resolve_alias_symbol(sym_id, visited);
        }

        let node = self.arena.get(idx)?;
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            if self.is_assignment_operator(bin.operator_token) {
                return self.reference_symbol_inner(bin.left, visited);
            }
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.arena.get_qualified_name(node)?;
            return self.resolve_namespace_member(qn.left, qn.right, visited);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            return self.resolve_namespace_member(
                access.expression,
                access.name_or_argument,
                visited,
            );
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name = self.literal_string_from_node(access.name_or_argument)?;
            return self.resolve_namespace_member_by_name(access.expression, name, visited);
        }

        None
    }

    pub(crate) fn resolve_namespace_member(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let right_name = self
            .arena
            .get(right)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;
        self.resolve_namespace_member_by_name(left, right_name, visited)
    }

    pub(crate) fn resolve_namespace_member_by_name(
        &self,
        left: NodeIndex,
        right_name: &str,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let left_sym = self.reference_symbol_inner(left, visited)?;
        let left_sym = self.resolve_alias_symbol(left_sym, visited)?;
        let left_symbol = self.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        let member_sym = exports.get(right_name)?;
        self.resolve_alias_symbol(member_sym, visited)
    }

    pub(crate) fn resolve_alias_symbol(
        &self,
        sym_id: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(sym_id);
        }
        if visited.contains(&sym_id) {
            return None;
        }
        visited.push(sym_id);

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import = self.arena.get_import_decl(decl_node)?;
        self.reference_symbol_inner(import.module_specifier, visited)
    }
}

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
            if let Some(ext) = self.arena.get_extended(decl_id) {
                if !ext.parent.is_none() {
                    if let Some(parent_node) = self.arena.get(ext.parent) {
                        let flags = parent_node.flags as u32;
                        let is_const = (flags & node_flags::CONST) != 0;
                        return !is_const; // Return true if NOT const (i.e., let or var)
                    }
                }
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
        use crate::binder::ScopeId;
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

        // Get the current scope (where the variable is being accessed)
        let current_scope_id = self.binder.current_scope_id;

        // If declared in current scope, not captured
        if decl_scope_id == current_scope_id {
            return false;
        }

        // Check if declaration scope is an ancestor of current scope
        // Walk up the scope chain from current scope to see if we find the declaration scope
        let mut scope_id = current_scope_id;
        let mut iterations = 0;
        while !scope_id.is_none() && iterations < MAX_TREE_WALK_ITERATIONS {
            if scope_id == decl_scope_id {
                // Found declaration scope in ancestor chain → captured variable
                return true;
            }

            // Move to parent scope
            scope_id = self
                .binder
                .scopes
                .get(scope_id.0 as usize)
                .map(|scope| scope.parent)
                .unwrap_or(ScopeId::NONE);

            iterations += 1;
        }

        false
    }

    /// Extract a TypeGuard from a condition node.
    ///
    /// This method translates AST nodes into AST-agnostic TypeGuard enums,
    /// which can then be passed to the Solver's `narrow_type()` method.
    ///
    /// Returns `Some((guard, target, is_optional))` where:
    /// - `guard` is the extracted TypeGuard
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
    #[allow(dead_code)]
    pub(crate) fn extract_type_guard(
        &self,
        condition: NodeIndex,
    ) -> Option<(TypeGuard, NodeIndex, bool)> {
        let cond_node = self.arena.get(condition)?;

        // Check for call expression (user-defined type guards) FIRST
        if cond_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            return self.extract_call_type_guard(condition);
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
        if let Some(type_name) = self.typeof_comparison_literal(bin.left, bin.right, target) {
            return Some((TypeGuard::Typeof(type_name.to_string()), target, false));
        }

        // Check for loose equality with null/undefined: x == null, x != null, x == undefined, x != undefined
        // TypeScript treats these as nullish equality (narrows to null | undefined)
        let is_loose_equality = bin.operator_token == SyntaxKind::EqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16;
        if is_loose_equality {
            if let Some(_nullish_type) = self.nullish_comparison(bin.left, bin.right, target) {
                // For loose equality with null/undefined, use NullishEquality guard
                // This narrows to null | undefined in true branch, excludes both in false
                return Some((TypeGuard::NullishEquality, target, false));
            }
        }

        // Check for strict nullish comparison: x === null, x !== null, x === undefined, x !== undefined
        if let Some(nullish_type) = self.nullish_comparison(bin.left, bin.right, target) {
            return Some((TypeGuard::LiteralEquality(nullish_type), target, false));
        }

        // Check for discriminant comparison: x.kind === "circle" or x.payload.kind === "circle"
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

        // Check for literal comparison: x === "foo", x === 42
        if let Some(literal_type) = self.literal_comparison(bin.left, bin.right, target) {
            return Some((TypeGuard::LiteralEquality(literal_type), target, false));
        }

        None
    }

    /// Extract a TypeGuard from a call expression (user-defined type guard).
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
        let node = self.arena.get(condition)?;
        let call = self.arena.get_call_expr(node)?;

        // Task 10: Check for Array.isArray(x) calls
        if let Some((guard, target)) = self.check_array_is_array(call, condition) {
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

    /// Check if a call expression uses optional chaining.
    ///
    /// For `obj?.method(x)`, `func?.()`, or `func?.(x)`, returns `true`.
    /// For `obj.method(x)`, returns `false`.
    fn is_optional_call(&self, call_node_idx: NodeIndex, call: &CallExprData) -> bool {
        // 1. Check if the call node itself has OptionalChain flag (e.g., func?.())
        if let Some(node) = self.arena.get(call_node_idx) {
            if (node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0 {
                return true;
            }
        }

        // 2. Check if the callee is a property access with ?. (e.g., obj?.method())
        if let Some(callee_node) = self.arena.get(call.expression) {
            if (callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.arena.get_access_expr(callee_node)
                && access.question_dot_token
            {
                return true;
            }
        }

        false
    }

    /// Get the target node being narrowed in a comparison expression.
    ///
    /// For `typeof x === "string"`, returns the node for `x`.
    /// For `x === null`, returns the node for `x`.
    fn get_comparison_target(&self, condition: NodeIndex) -> Option<NodeIndex> {
        let cond_node = self.arena.get(condition)?;
        let bin = self.arena.get_binary_expr(cond_node)?;

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

        Some(unary.operand)
    }
}
