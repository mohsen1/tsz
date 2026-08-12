//! Fallback type resolution helpers for assignment flow analysis.
//!
//! These functions derive approximate types from syntax and cached node types
//! when the full checker pipeline hasn't yet resolved a given expression.
//! Used by `get_assigned_type` in `assignment.rs` for flow-sensitive narrowing.

use super::FlowAnalyzer;
use crate::query_boundaries::flow_analysis::{
    PropertyAccessResult, TypeSubstitution, call_signatures_for_type,
    construct_signatures_for_type, contains_free_type_parameters, flow_call_signature,
    flow_property, function_return_type, get_application_info, get_lazy_def_id, instantiate_type,
    is_promise_like_type, literal_value, optional_flow_property, union_members_for_type,
    unwrap_promise_type_argument, widen_literal_to_primitive,
};
use crate::query_boundaries::operator_wrappers::is_equality_comparison_operator;
use crate::types_domain::queries::lib_resolution::{
    keyword_name_to_type_id, keyword_syntax_to_type_id,
};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{ParamInfo, SymbolRef, TypeId};

impl<'a> FlowAnalyzer<'a> {
    pub(super) fn assigned_type_for_await_rhs(
        &self,
        rhs: NodeIndex,
        rhs_type: TypeId,
    ) -> Option<TypeId> {
        let rhs_node = self.arena.get(rhs)?;
        if rhs_node.kind != syntax_kind_ext::AWAIT_EXPRESSION {
            return None;
        }

        // If the await node itself was cached as a promise-like application, unwrap once.
        if let Some(inner) = self.awaited_type_from_type(rhs_type) {
            return Some(inner);
        }
        if rhs_type != TypeId::ERROR {
            return Some(rhs_type);
        }

        // Fallback: derive from operand type (for cases where await-node cache
        // carries the pre-unwrapped promise-like type).
        let unary = self.arena.get_unary_expr_ex(rhs_node)?;
        let operand_type = self
            .node_types
            .and_then(|nt| nt.get(&unary.expression.0).copied())?;
        if let Some(inner) = self.awaited_type_from_type(operand_type) {
            return Some(inner);
        }
        (operand_type != TypeId::ERROR).then_some(operand_type)
    }

    pub(super) fn fallback_assigned_type_from_expression(&self, rhs: NodeIndex) -> Option<TypeId> {
        let rhs = self.skip_parens_and_assertions(rhs);
        let rhs_node = self.arena.get(rhs)?;

        if let Some(reference_type) = self.fallback_type_for_reference(rhs) {
            return Some(reference_type);
        }

        if rhs_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.arena.get_conditional_expr(rhs_node)
        {
            let consequent_type = self.literal_type_from_node(cond.when_true);
            let alternate_type = self.literal_type_from_node(cond.when_false);
            return match (consequent_type, alternate_type) {
                (Some(t), Some(f)) => Some(self.interner.union2(t, f)),
                (Some(t), None) | (None, Some(t)) => Some(t),
                _ => None,
            };
        }

        if rhs_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            return self.fallback_call_expression_type(rhs);
        }

        if rhs_node.kind == syntax_kind_ext::NEW_EXPRESSION {
            return self.fallback_new_expression_type(rhs);
        }

        if rhs_node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            return self.fallback_await_expression_type(rhs);
        }

        if rhs_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(property_type) = self.fallback_property_access_type(rhs)
        {
            return Some(property_type);
        }

        // Handle binary expressions whose types may only be in request_node_types
        // (contextually typed) rather than node_types. Compute the result type
        // from the operand types which ARE in node_types.
        if rhs_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(rhs_node)
        {
            return self.fallback_binary_expression_type(bin.left, bin.right, bin.operator_token);
        }

        // Array/object literal right-hand sides have no dedicated branch above and
        // are commonly uncached during return-type inference, where the inference
        // pass evaluates only return expressions (not the assignment statements in
        // sibling branches such as `if (!value) { value = ["a"]; }`). Route them
        // through the general syntax resolver so assignment-based flow narrowing of
        // a `let` binding produces the widened literal element type (matching the
        // cached read pass), instead of silently keeping the declared union type.
        if rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            return self.fallback_expression_type_from_syntax(rhs);
        }

        None
    }

    fn fallback_property_access_type(&self, expr: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(expr)?;
        let access = self.arena.get_access_expr(node)?;
        let prop_name = self
            .arena
            .get(access.name_or_argument)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| self.interner.intern_string(&ident.escaped_text))?;
        let receiver_type = self.fallback_expression_type_from_syntax(access.expression)?;
        crate::query_boundaries::property_access::resolve_property_access(
            self.interner,
            receiver_type,
            prop_name,
        )
        .success_type()
    }

    pub(super) fn fallback_expression_type_from_syntax(&self, expr: NodeIndex) -> Option<TypeId> {
        // Robustness audit (PR #K, item 11 in
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`): this is the
        // entry point of the syntax-driven fallback type resolver used by
        // flow-narrowing when the main checker pipeline hasn't yet typed
        // an expression. Trace each invocation so the rate at which
        // narrowing depends on the second resolver — and any divergence
        // from main-checker results — becomes observable.
        tracing::trace!(
            site = "flow::fallback_expression_type_from_syntax",
            expr_idx = expr.0,
            "flow-fallback resolver entered"
        );
        let expr = self.skip_parens_and_assertions(expr);
        if let Some(literal_type) = self.literal_type_from_node(expr) {
            return Some(widen_literal_to_primitive(self.interner, literal_type));
        }
        if let Some(nullish_type) = self.nullish_literal_type(expr) {
            return Some(nullish_type);
        }
        if let Some(ty) = self
            .node_types
            .and_then(|nt| nt.get(&expr.0).copied())
            .filter(|&ty| ty != TypeId::ERROR)
        {
            return Some(ty);
        }
        if let Some(reference_type) = self.fallback_type_for_reference(expr) {
            return Some(reference_type);
        }

        let expr_node = self.arena.get(expr)?;
        match expr_node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.fallback_array_literal_type_from_syntax(expr)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.fallback_object_literal_type_from_syntax(expr)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.fallback_property_access_type(expr)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(expr_node)?;
                self.fallback_binary_expression_type(
                    binary.left,
                    binary.right,
                    binary.operator_token,
                )
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => self.fallback_new_expression_type(expr),
            _ => None,
        }
    }

    fn fallback_type_from_type_node_syntax(&self, type_node: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(type_node)?;

        if let Some(ty) = self
            .node_types
            .and_then(|nt| nt.get(&type_node.0).copied())
            .filter(|&ty| ty != TypeId::ERROR)
        {
            return Some(ty);
        }

        if let Some(builtin) = keyword_syntax_to_type_id(node.kind) {
            return Some(builtin);
        }

        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => self
                .arena
                .get_wrapped_type(node)
                .and_then(|wrapped| self.fallback_type_from_type_node_syntax(wrapped.type_node)),
            k if k == syntax_kind_ext::LITERAL_TYPE => self
                .arena
                .get_literal_type(node)
                .and_then(|literal| self.literal_type_from_node(literal.literal)),
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut members = Vec::new();
                for &member in &composite.types.nodes {
                    members.push(self.fallback_type_from_type_node_syntax(member)?);
                }
                match members.len() {
                    0 => Some(TypeId::NEVER),
                    1 => members.first().copied(),
                    _ => Some(crate::query_boundaries::flow_analysis::union_types(
                        self.interner,
                        members,
                    )),
                }
            }
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut members = Vec::new();
                for &member in &composite.types.nodes {
                    members.push(self.fallback_type_from_type_node_syntax(member)?);
                }
                match members.len() {
                    0 => Some(TypeId::NEVER),
                    1 => members.first().copied(),
                    _ => Some(crate::query_boundaries::flow_analysis::intersection_types(
                        self.interner,
                        members,
                    )),
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                let array = self.arena.get_array_type(node)?;
                let elem = self.fallback_type_from_type_node_syntax(array.element_type)?;
                Some(crate::query_boundaries::flow_analysis::array_type(
                    self.interner,
                    elem,
                ))
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                // Resolve an inline object type annotation (`{ v: string }`) so a
                // reference whose declared type is a structural object resolves to a
                // real object type the property-access fallback can read, instead of
                // an opaque `Lazy`. Conservative: bail to `None` (keep the `Lazy`) if
                // any member is not a plain typed property signature, so partial or
                // unsupported shapes never produce a structurally wrong object type.
                let type_literal = self.arena.get_type_literal(node)?;
                let mut properties = Vec::with_capacity(type_literal.members.nodes.len());
                for &member in &type_literal.members.nodes {
                    let member_node = self.arena.get(member)?;
                    if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                        return None;
                    }
                    let signature = self.arena.get_signature(member_node)?;
                    if signature.type_annotation.is_none() {
                        return None;
                    }
                    let name_atom = self.fallback_object_property_name_atom(signature.name)?;
                    let member_type =
                        self.fallback_type_from_type_node_syntax(signature.type_annotation)?;
                    properties.push(if signature.question_token {
                        optional_flow_property(name_atom, member_type)
                    } else {
                        flow_property(name_atom, member_type)
                    });
                }
                Some(
                    crate::query_boundaries::flow_analysis::object_type_from_properties(
                        self.interner,
                        properties,
                    ),
                )
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                // Primitive keyword types (`string`, `number`, `boolean`, …) are
                // parsed as bare type references whose name is a reserved keyword,
                // not as dedicated keyword type nodes. They carry no binder symbol,
                // so the symbol-resolution path below would bail and the syntactic
                // fallback would return `None` whenever the annotation node has not
                // been cached in `node_types`. Map the reserved keyword name to its
                // built-in `TypeId` directly. These names cannot be user-declared,
                // so this is a true-builtin lookup, not an identifier-string heuristic.
                if type_ref.type_arguments.is_none()
                    && let Some(name) = self.arena.get_identifier_at(type_ref.type_name)
                    && let Some(builtin) = keyword_name_to_type_id(&name.escaped_text)
                {
                    return Some(builtin);
                }
                let sym_id = self
                    .binder
                    .resolve_identifier(self.arena, type_ref.type_name)
                    .or_else(|| self.reference_symbol(type_ref.type_name))?;
                let symbol = self.binder.get_symbol(sym_id)?;
                // Type parameters (e.g., K in `new Set<K>()`) need special handling:
                // look up the type parameter's TypeId from node_types for its declaration.
                // Without this, generic new expressions with type parameter arguments
                // fail the fallback path, losing the type argument information.
                if (symbol.flags & tsz_binder::symbol_flags::TYPE_PARAMETER) != 0 {
                    return symbol.declarations.iter().copied().find_map(|decl| {
                        self.node_types
                            .and_then(|nt| nt.get(&decl.0).copied())
                            .filter(|&ty| ty != TypeId::ERROR)
                    });
                }
                let base_type = symbol
                    .declarations
                    .iter()
                    .copied()
                    .find_map(|decl| self.fallback_named_type_declaration_type(decl));
                // For generic type references with type arguments (e.g., Box<K>),
                // apply the type arguments to the base type.
                if let Some(base) = base_type
                    && let Some(ref type_args_list) = type_ref.type_arguments
                    && !type_args_list.nodes.is_empty()
                {
                    let mut resolved_args = Vec::with_capacity(type_args_list.nodes.len());
                    for &arg_idx in &type_args_list.nodes {
                        resolved_args.push(self.fallback_type_from_type_node_syntax(arg_idx)?);
                    }
                    Some(self.interner.application(base, resolved_args))
                } else {
                    base_type
                }
            }
            _ => None,
        }
    }

    fn fallback_named_type_declaration_type(&self, decl: NodeIndex) -> Option<TypeId> {
        if let Some(ty) = self
            .node_types
            .and_then(|nt| nt.get(&decl.0).copied())
            .filter(|&ty| ty != TypeId::ERROR)
        {
            return Some(ty);
        }

        let node = self.arena.get(decl)?;
        match node.kind {
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let alias = self.arena.get_type_alias(node)?;
                self.node_types
                    .and_then(|nt| nt.get(&alias.type_node.0).copied())
                    .filter(|&ty| ty != TypeId::ERROR)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION =>
            {
                self.node_types
                    .and_then(|nt| nt.get(&decl.0).copied())
                    .filter(|&ty| ty != TypeId::ERROR)
            }
            _ => None,
        }
    }

    fn fallback_array_literal_type_from_syntax(&self, expr: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(expr)?;
        let literal = self.arena.get_literal_expr(node)?;
        let mut element_types = Vec::with_capacity(literal.elements.nodes.len());

        for &element in &literal.elements.nodes {
            if element.is_none() {
                continue;
            }
            let element = self.skip_parens_and_assertions(element);
            let element_node = self.arena.get(element)?;
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                return None;
            }
            element_types.push(self.fallback_expression_type_from_syntax(element)?);
        }

        let element_type = match element_types.len() {
            0 => TypeId::NEVER,
            1 => element_types[0],
            _ => crate::query_boundaries::flow_analysis::union_types(self.interner, element_types),
        };
        Some(crate::query_boundaries::flow_analysis::array_type(
            self.interner,
            element_type,
        ))
    }

    fn fallback_object_literal_type_from_syntax(&self, expr: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(expr)?;
        let literal = self.arena.get_literal_expr(node)?;
        let mut properties = Vec::with_capacity(literal.elements.nodes.len());

        for &element in &literal.elements.nodes {
            if element.is_none() {
                continue;
            }
            let element_node = self.arena.get(element)?;
            if let Some(prop) = self.arena.get_property_assignment(element_node) {
                let name_atom = self.fallback_object_property_name_atom(prop.name)?;
                // Preserve literal types in object literal properties (don't widen).
                // This allows accurate structural comparison during assignment narrowing.
                let value_type = self
                    .literal_type_from_node(prop.initializer)
                    .or_else(|| self.fallback_expression_type_from_syntax(prop.initializer))?;
                properties.push(flow_property(name_atom, value_type));
                continue;
            }
            if let Some(shorthand) = self.arena.get_shorthand_property(element_node) {
                let name_node = self.arena.get(shorthand.name)?;
                let ident = self.arena.get_identifier(name_node)?;
                let value_type = self
                    .node_types
                    .and_then(|nt| nt.get(&shorthand.name.0).copied())
                    .or_else(|| self.fallback_type_for_reference(shorthand.name))?;
                // Re-intern through solver to match solver's Atom namespace.
                let name_atom = self.interner.intern_string(&ident.escaped_text);
                properties.push(flow_property(name_atom, value_type));
                continue;
            }
            return None;
        }

        Some(
            crate::query_boundaries::flow_analysis::object_type_from_properties(
                self.interner,
                properties,
            ),
        )
    }

    fn fallback_object_property_name_atom(&self, name_idx: NodeIndex) -> Option<Atom> {
        let name_node = self.arena.get(name_idx)?;
        if let Some(ident) = self.arena.get_identifier(name_node) {
            // Re-intern through solver to ensure Atom namespace matches solver types.
            // Scanner-interned atoms (ident.atom) use a different interner than the solver.
            return Some(self.interner.intern_string(&ident.escaped_text));
        }
        if let Some(literal) = self.arena.get_literal(name_node) {
            return Some(self.interner.intern_string(&literal.text));
        }
        if let Some(computed) = self.arena.get_computed_property(name_node) {
            let key_type = self.fallback_expression_type_from_syntax(computed.expression)?;
            if let Some(literal) = literal_value(self.interner, key_type) {
                return Some(match literal {
                    tsz_solver::LiteralValue::Number(value) => self
                        .interner
                        .intern_string(&tsz_solver::utils::js_number_to_string(value.0)),
                    tsz_solver::LiteralValue::Boolean(value) => self
                        .interner
                        .intern_string(if value { "true" } else { "false" }),
                    tsz_solver::LiteralValue::String(atom)
                    | tsz_solver::LiteralValue::BigInt(atom) => atom,
                });
            }
        }
        None
    }

    /// Drop a syntactic-fallback result that still carries a free
    /// (un-instantiated) type parameter.
    ///
    /// The syntactic flow fallback performs no type-argument inference, so a
    /// generic callee's declared return type still references the signature's own
    /// type parameters (e.g. `pipe<A, B>(a: A, ab: (a: A) => B): B`). Surfacing
    /// that bare parameter would leak it into the caller's flow type: a
    /// self-referential `x = f(x)` loop back-edge then re-checks the call argument
    /// against the leaked parameter and falsely reports TS2345. Declining keeps
    /// the caller at its prior/declared flow type so the main inference pipeline
    /// resolves the call result.
    fn reject_unresolved_generic_result(&self, ty: Option<TypeId>) -> Option<TypeId> {
        ty.filter(|&ty| !contains_free_type_parameters(self.interner, ty))
    }

    /// Returns `true` when `expr` is a syntactic form that always evaluates to a
    /// definitely non-nullish value, regardless of how (or whether) the full
    /// checker pipeline managed to cache its type.
    ///
    /// These node kinds construct a fresh value on evaluation — an object/array
    /// literal, a `new` expression, or a function/arrow/class expression — so the
    /// result can never be `null` or `undefined`. Call expressions are excluded:
    /// a call's return type is not knowable from syntax alone and may itself be
    /// nullable (e.g. `t = maybeUndef()`), so killing-definition narrowing for
    /// calls stays on the type-driven path.
    pub(super) fn is_syntactically_non_nullish_expression(&self, expr: NodeIndex) -> bool {
        let expr = self.skip_parens_and_assertions(expr);
        let Some(node) = self.arena.get(expr) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION
        )
    }

    fn fallback_call_expression_type(&self, call_expr: NodeIndex) -> Option<TypeId> {
        self.reject_unresolved_generic_result(self.fallback_call_expression_type_inner(call_expr))
            .or_else(|| self.fallback_instantiated_generic_call_type(call_expr))
    }

    /// Recover a concrete return for the one unambiguous generic-call shape.
    /// Call selection, inference, constraints, and return instantiation stay in
    /// the solver's canonical call resolver; flow only supplies cached argument
    /// types and fails closed for explicit type arguments or spread syntax.
    fn fallback_instantiated_generic_call_type(&self, call_expr: NodeIndex) -> Option<TypeId> {
        let call_node = self.arena.get(call_expr)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION || call_node.is_optional_chain() {
            return None;
        }

        let call = self.arena.get_call_expr(call_node)?;
        let callee = self.skip_parens_and_assertions(call.expression);
        let callee_type = self
            .fallback_cached_callable_reference_type(callee)
            .or_else(|| self.fallback_type_for_reference(callee))
            .or_else(|| self.fallback_expression_type_from_syntax(callee))
            .map(|ty| self.resolve_lazy_via_env(ty))?;
        if call.type_arguments.is_some() {
            return None;
        }

        let arguments = call
            .arguments
            .as_ref()
            .map_or(&[][..], |arguments| arguments.nodes.as_slice());
        let node_types = self.node_types?;
        let mut argument_types = Vec::with_capacity(arguments.len());
        for argument in arguments.iter().copied() {
            let argument_node = self.arena.get(argument)?;
            if argument_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                return None;
            }
            let stripped_argument = self.skip_parens_and_assertions(argument);
            let argument_type = node_types
                .get(&argument.0)
                .or_else(|| node_types.get(&stripped_argument.0))
                .copied()
                .filter(|&ty| ty != TypeId::ERROR)
                .or_else(|| self.fallback_expression_type_from_syntax(stripped_argument))
                .filter(|&ty| ty != TypeId::ERROR)
                .map(|ty| self.resolve_lazy_via_env(ty))?;
            argument_types.push(argument_type);
        }

        let ctx = self.checker_context?;
        let env = self.type_environment?.borrow();
        let return_type =
            crate::query_boundaries::checkers::call::resolve_single_non_rest_generic_call_with_context(
                self.interner,
                ctx,
                &env,
                callee_type,
                &argument_types,
            )?;
        self.reject_unresolved_generic_result((return_type != TypeId::ERROR).then_some(return_type))
    }

    fn fallback_call_expression_type_inner(&self, call_expr: NodeIndex) -> Option<TypeId> {
        let call_node = self.arena.get(call_expr)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(call_node)?;
        let callee = self.skip_parens_and_assertions(call.expression);
        if let Some(callee_type) = self.fallback_type_for_reference(callee)
            && let Some(return_type) = self.call_return_type_from_type(callee_type)
        {
            return Some(return_type);
        }

        let sym_id = self.reference_symbol(callee)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        let mut return_types = Vec::with_capacity(symbol.declarations.len());
        for &decl in &symbol.declarations {
            if let Some(return_type) = self.declared_return_type_from_declaration(decl) {
                return_types.push(return_type);
                continue;
            }
            if let Some(decl_type) = self.fallback_declaration_type(decl) {
                self.extend_call_return_types(decl_type, &mut return_types);
            }
        }
        self.union_types_if_any(return_types)
    }

    fn fallback_new_expression_type(&self, new_expr: NodeIndex) -> Option<TypeId> {
        // A generic constructor invoked without explicit type arguments leaves the
        // signature's own type parameters free in the declared return type; reuse
        // the same guard as the call fallback rather than leak them.
        self.reject_unresolved_generic_result(self.fallback_new_expression_type_inner(new_expr))
    }

    fn fallback_new_expression_type_inner(&self, new_expr: NodeIndex) -> Option<TypeId> {
        let new_node = self.arena.get(new_expr)?;
        if new_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(new_node)?;
        let callee = self.skip_parens_and_assertions(call.expression);
        let ctor_type = self
            .fallback_type_for_reference(callee)
            .or_else(|| self.fallback_expression_type_from_syntax(callee))
            .map(|ty| self.resolve_lazy_via_env(ty))?;

        let signatures = construct_signatures_for_type(self.interner, ctor_type)?;
        if signatures.is_empty() {
            return None;
        }

        let mut explicit_type_args: smallvec::SmallVec<[TypeId; 4]> = smallvec::SmallVec::new();
        if let Some(type_arguments) = call.type_arguments.as_ref() {
            for &arg_idx in &type_arguments.nodes {
                explicit_type_args.push(self.fallback_type_from_type_node_syntax(arg_idx)?);
            }
        }

        let mut return_types = Vec::with_capacity(signatures.len());
        for sig in signatures {
            let return_type = if explicit_type_args.is_empty() || sig.type_params.is_empty() {
                sig.return_type
            } else {
                let mut applied_args: smallvec::SmallVec<[TypeId; 4]> = explicit_type_args
                    .iter()
                    .copied()
                    .take(sig.type_params.len())
                    .collect();
                if applied_args.len() < sig.type_params.len() {
                    for param in sig.type_params.iter().skip(applied_args.len()) {
                        applied_args.push(
                            param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN),
                        );
                    }
                }
                let substitution = TypeSubstitution::from_signature_args(
                    self.interner,
                    &sig.type_params,
                    &applied_args,
                );
                instantiate_type(self.interner, sig.return_type, &substitution)
            };
            return_types.push(return_type);
        }

        self.union_types_if_any(return_types)
    }

    fn fallback_await_expression_type(&self, await_expr: NodeIndex) -> Option<TypeId> {
        let await_node = self.arena.get(await_expr)?;
        if await_node.kind != syntax_kind_ext::AWAIT_EXPRESSION {
            return None;
        }

        let unary = self.arena.get_unary_expr_ex(await_node)?;
        let operand = self.skip_parens_and_assertions(unary.expression);
        if let Some(awaited_call_type) = self.fallback_awaited_call_expression_type(operand) {
            return Some(awaited_call_type);
        }
        let operand_type = self
            .fallback_assigned_type_from_expression(operand)
            .or_else(|| self.node_types.and_then(|nt| nt.get(&operand.0).copied()))?;
        self.awaited_type_from_type(operand_type)
            .or(Some(operand_type))
    }

    fn fallback_awaited_call_expression_type(&self, operand: NodeIndex) -> Option<TypeId> {
        let operand = self.skip_parens_and_assertions(operand);
        let call_node = self.arena.get(operand)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(call_node)?;
        let callee = self.skip_parens_and_assertions(call.expression);
        let sym_id = self.reference_symbol(callee)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if !symbol
            .declarations
            .iter()
            .copied()
            .any(|decl| self.declaration_likely_returns_awaitable(decl))
        {
            return None;
        }

        let call_type = self
            .fallback_call_expression_type(operand)
            .or_else(|| self.node_types.and_then(|nt| nt.get(&operand.0).copied()))?;

        if let Some((_, args)) = get_application_info(self.interner, call_type)
            && let Some(&first_arg) = args.first()
        {
            return Some(first_arg);
        }

        let members = union_members_for_type(self.interner, call_type)?;
        let mut awaited_types = Vec::with_capacity(members.len());
        for member in members {
            let (.., args) = get_application_info(self.interner, member)?;
            awaited_types.push(*args.first()?);
        }
        self.union_types_if_any(awaited_types)
    }

    fn awaited_type_from_type(&self, ty: TypeId) -> Option<TypeId> {
        if let Some(inner) = unwrap_promise_type_argument(self.interner, ty) {
            return Some(inner);
        }
        if let Some((base, args)) = get_application_info(self.interner, ty)
            && let Some(&first_arg) = args.first()
        {
            if base == TypeId::PROMISE_BASE {
                return Some(first_arg);
            }

            let resolved_base = self.resolve_lazy_via_env(base);
            if resolved_base != base && is_promise_like_type(self.interner, resolved_base) {
                return Some(first_arg);
            }
        }

        let members = union_members_for_type(self.interner, ty)?;
        let mut awaited_members = Vec::with_capacity(members.len());
        for member in members {
            awaited_members.push(unwrap_promise_type_argument(self.interner, member)?);
        }

        match awaited_members.len() {
            0 => None,
            1 => awaited_members.first().copied(),
            _ => Some(crate::query_boundaries::flow_analysis::union_types(
                self.interner,
                awaited_members,
            )),
        }
    }

    fn fallback_type_for_reference(&self, reference: NodeIndex) -> Option<TypeId> {
        let reference = self.skip_parens_and_assertions(reference);
        if let Some(ty) = self
            .node_types
            .and_then(|nt| nt.get(&reference.0).copied())
            .filter(|&ty| ty != TypeId::ERROR)
        {
            return Some(ty);
        }

        let sym_id = self.reference_symbol(reference)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        let decl = symbol.primary_declaration()?;
        let declared_type = self
            .annotation_type_from_var_decl_node(decl)
            .or_else(|| {
                self.node_types
                    .and_then(|nt| nt.get(&decl.0).copied())
                    .filter(|&ty| ty != TypeId::ERROR)
            })
            .or_else(|| {
                // Contextually typed callback parameters are published on their
                // symbol before deferred body flow runs, even when speculative
                // return inference has rolled back the per-node cache. Reuse that
                // concrete declared/contextual type before manufacturing a Lazy
                // fallback, whose body is not registered during the provisional
                // walk and cannot participate in structural argument inference.
                self.arena
                    .get(decl)
                    .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
                    .then(|| self.fallback_cached_stable_symbol_type(sym_id))
                    .flatten()
            })
            .or_else(|| {
                self.resolve_symbol_to_lazy(SymbolRef(sym_id.0))
                    .map(|ty| self.resolve_lazy_via_env(ty))
            });
        // Recover from an unresolved `Lazy` declared type. A parameter or
        // block-scoped local whose declaration statement has not been checked in
        // this pass (e.g. during inferred-return inference, which evaluates only
        // return expressions and `if` conditions — not sibling assignment
        // statements) resolves its symbol to a `Lazy(DefId)` that the
        // `TypeEnvironment` cannot resolve here, so it leaks out opaque. An opaque
        // `Lazy` is "subtype of nothing" for `narrow_assignment`, so an assignment
        // RHS that reads such a reference (`x = param`, `x = local`) would silently
        // contribute nothing to the flow type and the declared union would survive.
        // Read the declaration's syntactic type annotation directly to recover the
        // concrete declared type without depending on `node_types`/env priming.
        let declared_type = match declared_type {
            Some(ty) if self.is_unresolved_lazy_type(ty) => {
                self.fallback_declared_annotation_type(decl).or(Some(ty))
            }
            other => other,
        };
        if let (Some(initial_type), Some(flow_node)) =
            (declared_type, self.binder.get_node_flow(reference))
        {
            let flowed = self.get_flow_type(reference, initial_type, flow_node);
            if flowed != TypeId::ERROR {
                return Some(flowed);
            }
        }
        declared_type.or_else(|| self.fallback_declaration_type(decl))
    }

    /// Read a stable declared/contextual symbol type already published by the
    /// checker. Semantic `any` is usable here once symbol resolution has
    /// completed: TypeScript infers through it and a non-null generic result still
    /// kills `undefined`. In-flight entries, `unknown`, error sentinels, unresolved
    /// lazy refs, and free parameters are incomplete evidence.
    fn fallback_cached_stable_symbol_type(&self, sym_id: SymbolId) -> Option<TypeId> {
        let ctx = self.checker_context?;
        if ctx.symbol_resolution_set.contains(&sym_id) {
            return None;
        }
        let ty = ctx.symbol_types.get(&sym_id)?;
        let ty = self.resolve_lazy_via_env(ty);
        (!matches!(ty, TypeId::ERROR | TypeId::UNKNOWN)
            && !self.is_unresolved_lazy_type(ty)
            && !contains_free_type_parameters(self.interner, ty))
        .then_some(ty)
    }

    /// Read a callable symbol type without rejecting its signature-scoped type
    /// parameters as free. Imported generic functions have no local function
    /// declaration for the syntax fallback, but their checked alias symbol type
    /// is stable and carries the call signature needed for ordinary inference.
    fn fallback_cached_callable_reference_type(&self, reference: NodeIndex) -> Option<TypeId> {
        let reference = self.skip_parenthesized(reference);
        // Preserve the raw symbol bound in this file before `reference_symbol`
        // follows imports. Per-file binders mint colliding raw `SymbolId`s, so a
        // terminal foreign id must never be inspected through the current binder.
        let local_sym_id = self.binder.get_node_symbol(reference).or_else(|| {
            self.binder
                .resolve_identifier_with_filter(self.arena, reference, &[], |_| true)
        });
        let sym_id = self.reference_symbol(reference)?;
        let ctx = self.checker_context?;
        let local_is_alias = local_sym_id
            .and_then(|local| self.binder.get_symbol(local))
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS));
        let (target, target_file_idx) = if local_is_alias {
            let (target, owner) = local_sym_id
                .and_then(|local| ctx.resolve_import_alias_chain_with_owner_and_register(local))?;
            (target, Some(owner))
        } else {
            let owner = if local_sym_id == Some(sym_id) {
                Some(ctx.current_file_idx)
            } else {
                ctx.resolve_dynamic_symbol_file_index(sym_id)
                    .or_else(|| ctx.resolve_symbol_file_index_stable(sym_id))
            };
            (sym_id, owner)
        };
        let cross_file = target_file_idx
            .and_then(|file_idx| ctx.cached_cross_file_symbol_type(target, file_idx as u32));
        // Interactive/LSP contexts intentionally disable persistent owner-cache
        // sharing. Their cross-arena checker still publishes the canonical
        // declaration body through the owner-keyed `DefId` environment, which
        // is safe to read here without consulting any raw-`SymbolId` cache.
        let owner_def_type = target_file_idx
            .and_then(|file_idx| {
                ctx.definition_store
                    .lookup_by_symbol(target.0, file_idx as u32)
            })
            .and_then(|def_id| {
                self.type_environment
                    .and_then(|env| env.borrow().get_def(def_id))
            });
        let target_env_type = self
            .type_environment
            .and_then(|env| env.borrow().get(SymbolRef(target.0)));
        let target_is_current = target_file_idx == Some(ctx.current_file_idx);
        let ty = cross_file
            .map(|(ty, _)| ty)
            .or(owner_def_type)
            .or_else(|| target_is_current.then_some(target_env_type).flatten())?;
        let ty = self.resolve_lazy_via_env(ty);
        (!matches!(ty, TypeId::ERROR | TypeId::UNKNOWN) && !self.is_unresolved_lazy_type(ty))
            .then_some(ty)
    }

    /// True when `ty` is a `Lazy(DefId)` that no `TypeEnvironment` resolution has
    /// collapsed to a concrete type. Such a type carries no structural shape, so
    /// flow narrowing (`narrow_assignment`, truthiness filtering) cannot reduce a
    /// union against it; callers must recover a concrete declared type instead.
    fn is_unresolved_lazy_type(&self, ty: TypeId) -> bool {
        get_lazy_def_id(self.interner.as_type_database(), ty).is_some()
    }

    /// Resolve a declaration's declared type from its syntactic type annotation.
    ///
    /// Handles parameters and variable declarations — the binding forms whose
    /// `Lazy(DefId)` can leak unresolved during inferred-return inference. Returns
    /// `None` when the declaration has no annotation or the annotation syntax is
    /// not one the syntactic resolver understands (callers then keep the `Lazy`).
    pub(super) fn fallback_declared_annotation_type(&self, decl: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(decl)?;
        let annotation = match node.kind {
            k if k == syntax_kind_ext::PARAMETER => self.arena.get_parameter(node)?.type_annotation,
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.arena.get_variable_declaration(node)?.type_annotation
            }
            _ => return None,
        };
        if annotation.is_none() {
            return None;
        }
        self.fallback_type_from_type_node_syntax(annotation)
    }

    fn fallback_declaration_type(&self, decl: NodeIndex) -> Option<TypeId> {
        self.annotation_type_from_var_decl_node(decl)
            .or_else(|| self.node_types.and_then(|nt| nt.get(&decl.0).copied()))
            .or_else(|| self.fallback_function_declaration_type(decl))
    }

    fn fallback_function_declaration_type(&self, decl: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(decl)?;
        let parameters = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                self.arena.get_function(node).map(|func| &func.parameters)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(node)
                .map(|method| &method.parameters),
            _ => None,
        }?;

        let mut params = Vec::new();
        let mut this_type = None;
        for &param_idx in &parameters.nodes {
            let param = self.arena.get_parameter_at(param_idx)?;
            let param_name = self
                .arena
                .get_identifier_at(param.name)
                .map(|ident| ident.escaped_text.as_str());
            let param_type = if param.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.node_types
                    .and_then(|nt| nt.get(&param.type_annotation.0).copied())
                    .filter(|&ty| ty != TypeId::ERROR)
                    .or_else(|| self.fallback_type_from_type_node_syntax(param.type_annotation))
                    .unwrap_or(TypeId::ANY)
            };
            if param_name == Some("this") {
                this_type = Some(param_type);
                continue;
            }
            params.push(ParamInfo {
                name: param_name.map(|name| self.interner.intern_string(name)),
                type_id: param_type,
                optional: param.question_token || param.initializer.is_some(),
                rest: param.dot_dot_dot_token,
                arity_only_optional: false,
            });
        }

        Some(
            crate::query_boundaries::flow_analysis::call_only_callable_type(
                self.interner,
                vec![flow_call_signature(
                    params,
                    this_type,
                    self.arena
                        .get(decl)
                        .and_then(|node| self.arena.get_function(node))
                        .and_then(|func| {
                            func.type_annotation
                                .is_some()
                                .then_some(func.type_annotation)
                        })
                        .and_then(|type_ann| self.fallback_type_from_type_node_syntax(type_ann))
                        .unwrap_or(TypeId::ANY),
                )],
            ),
        )
    }

    fn declared_return_type_from_declaration(&self, decl: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(decl)?;
        let func = self.arena.get_function(node)?;
        if func.type_annotation.is_none() {
            return None;
        }
        self.node_types
            .and_then(|nt| nt.get(&func.type_annotation.0).copied())
            .filter(|&ty| ty != TypeId::ERROR)
            .or_else(|| self.fallback_type_from_type_node_syntax(func.type_annotation))
    }

    fn declaration_likely_returns_awaitable(&self, decl: NodeIndex) -> bool {
        self.declaration_is_async_function(decl)
            || self
                .arena
                .get(decl)
                .and_then(|node| self.arena.get_function(node))
                .is_some_and(|func| {
                    func.type_annotation.is_some()
                        && self.type_annotation_is_awaitable(func.type_annotation)
                })
    }

    fn declaration_is_async_function(&self, decl: NodeIndex) -> bool {
        self.arena
            .get(decl)
            .and_then(|node| self.arena.get_function(node))
            .is_some_and(|func| func.is_async)
    }

    fn type_annotation_is_awaitable(&self, type_annotation: NodeIndex) -> bool {
        let Some(annotation_type) = self
            .node_types
            .and_then(|nt| nt.get(&type_annotation.0).copied())
            .filter(|&ty| ty != TypeId::ERROR)
            .or_else(|| self.fallback_type_from_type_node_syntax(type_annotation))
        else {
            return false;
        };

        self.awaited_type_from_type(annotation_type).is_some()
    }

    fn call_return_type_from_type(&self, ty: TypeId) -> Option<TypeId> {
        let mut return_types = Vec::new();
        self.extend_call_return_types(self.resolve_lazy_via_env(ty), &mut return_types);
        self.union_types_if_any(return_types)
    }

    fn extend_call_return_types(&self, ty: TypeId, return_types: &mut Vec<TypeId>) {
        if let Some(signatures) = call_signatures_for_type(self.interner, ty)
            && !signatures.is_empty()
        {
            return_types.extend(signatures.iter().map(|sig| sig.return_type));
            return;
        }

        if let Some(return_type) = function_return_type(self.interner, ty) {
            return_types.push(return_type);
        }
    }

    /// Compute the result type of a binary expression from its operand types.
    ///
    /// Used as a fallback when the binary expression's type is only in
    /// `request_node_types` (contextually typed) and not in `node_types`.
    /// This handles `??`, `||`, and `+` which are the most common cases
    /// where an assignment RHS is a binary expression whose cached type
    /// is missing from the non-contextual cache.
    fn fallback_binary_expression_type(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        operator: u16,
    ) -> Option<TypeId> {
        // Equality expressions always have boolean result type. Operand checking
        // and any comparison diagnostic remain owned by the ordinary checker;
        // flow only needs the structural result while reconstructing an uncached
        // object-literal argument.
        if is_equality_comparison_operator(operator) {
            return Some(TypeId::BOOLEAN);
        }
        if operator == SyntaxKind::QuestionQuestionToken as u16 {
            // x ?? y -> NonNullable<typeof x> | typeof y
            let left_type = self.resolve_operand_type(left)?;
            let right_type = self.resolve_operand_type(right)?;
            let non_nullish_left = self.interner.remove_nullish(left_type);
            let result = self.interner.union2(non_nullish_left, right_type);
            self.interner
                .replace_union_origin_for_display(result, vec![right_type, non_nullish_left]);
            return Some(result);
        }
        if operator == SyntaxKind::BarBarToken as u16 {
            // x || y -> typeof y | NonNullable<typeof x>
            // TypeScript narrows the left side in || result types: the truthy branch
            // removes null/undefined (and other falsy types, but removing nullish covers
            // the most important case for flow analysis). Keep the same display order
            // as the main binary evaluator so diagnostics retain the whole-expression
            // surface for type parameters.
            let left_type = self.resolve_operand_type(left)?;
            let right_type = self.resolve_operand_type(right)?;
            let non_nullish_left = self.interner.remove_nullish(left_type);
            return Some(self.interner.union2(right_type, non_nullish_left));
        }
        if operator == SyntaxKind::PlusToken as u16 {
            // If either operand is string, result is string
            let left_type = self.resolve_operand_type(left);
            let right_type = self.resolve_operand_type(right);
            if left_type == Some(TypeId::STRING) || right_type == Some(TypeId::STRING) {
                return Some(TypeId::STRING);
            }
        }
        None
    }

    /// Resolve the type of an expression operand using `node_types` cache,
    /// literal detection, or reference resolution.
    pub(super) fn resolve_operand_type(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parens_and_assertions(idx);
        // Try node_types first
        if let Some(ty) = self.node_types.and_then(|nt| nt.get(&idx.0).copied()) {
            return Some(ty);
        }
        // Try literal type
        if let Some(literal_type) = self.literal_type_from_node(idx) {
            return Some(literal_type);
        }
        // Try reference resolution
        if let Some(reference_type) = self.fallback_type_for_reference(idx) {
            return Some(reference_type);
        }
        None
    }

    pub(super) fn union_types_if_any(&self, mut types: Vec<TypeId>) -> Option<TypeId> {
        match types.len() {
            0 => None,
            1 => types.pop(),
            _ => Some(crate::query_boundaries::flow_analysis::union_types(
                self.interner,
                types,
            )),
        }
    }

    /// Resolve the property accessed by an access-reference `target` against its
    /// receiver's type and return the raw [`PropertyAccessResult`]. The receiver
    /// type is the cached node type, or `concrete_this_type` for a bare `this`.
    /// Returns `None` for a computed/dynamic key or an unresolved receiver.
    /// Shared by the read/write-surface split and the declared-reference-type
    /// query so the name/base/resolution logic lives in one place.
    pub(super) fn resolve_access_reference_property(
        &self,
        target: NodeIndex,
    ) -> Option<PropertyAccessResult> {
        let target_node = self.arena.get(target)?;
        let access = self.arena.get_access_expr(target_node)?;

        let name_atom = if target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let ident = self.arena.get_identifier_at(access.name_or_argument)?;
            self.interner.intern_string(&ident.escaped_text)
        } else if target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            self.literal_atom_from_node_or_type(access.name_or_argument)?
        } else {
            return None;
        };

        let node_types = self.node_types?;
        let base_type = if let Some(&base_type) = node_types.get(&access.expression.0) {
            base_type
        } else if let Some(this_type) = self.concrete_this_type
            && let Some(base_node) = self.arena.get(access.expression)
            && base_node.kind == SyntaxKind::ThisKeyword as u16
        {
            this_type
        } else {
            return None;
        };

        Some(if let Some(env_ref) = &self.type_environment {
            let env = env_ref.borrow();
            crate::query_boundaries::property_access::resolve_property_access_with_resolver(
                self.interner,
                &*env,
                base_type,
                name_atom,
                self.interner.no_unchecked_indexed_access(),
            )
        } else {
            crate::query_boundaries::property_access::resolve_property_access_with_options(
                self.interner,
                base_type,
                name_atom,
                self.interner.no_unchecked_indexed_access(),
            )
        })
    }

    /// True when `node` is an object- or array-literal expression — the RHS
    /// shape whose fresh structure drops declared property modifiers.
    pub(super) fn node_is_object_or_array_literal(&self, node: NodeIndex) -> bool {
        self.arena.get(node).is_some_and(|n| {
            n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        })
    }

    /// The *declared* type of a property/element access reference: the property
    /// resolved on the receiver's type, evaluated through the environment so an
    /// alias / mapped / `Readonly<…>` application exposes its concrete property
    /// modifiers. Returns `None` for a computed/dynamic key or an unresolved
    /// receiver. Used to reduce an object-literal assignment against the
    /// declared shape rather than adopting the literal's fresh (modifier-less)
    /// structure.
    pub(super) fn declared_access_reference_type(&self, target: NodeIndex) -> Option<TypeId> {
        match self.resolve_access_reference_property(target)? {
            PropertyAccessResult::Success { type_id, .. } => Some(self.evaluated_via_env(type_id)),
            _ => None,
        }
    }

    /// Resolve a type to the structural form whose property modifiers can be
    /// classified: resolve a `Lazy(DefId)` through the `TypeEnvironment`, then
    /// evaluate an alias / mapped application (e.g. `Readonly<{ … }>`).
    fn evaluated_via_env(&self, type_id: TypeId) -> TypeId {
        let resolved = self.resolve_lazy_via_env(type_id);
        if let Some(env) = &self.type_environment {
            let env = env.borrow();
            return crate::query_boundaries::flow_analysis::evaluate_application_type(
                self.interner,
                &env,
                resolved,
            );
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::FlowAnalyzer;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::{TypeId, construction::TypeInterner};

    #[test]
    fn stable_symbol_fallback_rejects_in_flight_any_sentinel() {
        let mut parser = ParserState::new("test.ts".to_string(), "const payload = 1;".to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let symbol = binder.file_locals.get("payload").expect("payload symbol");
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            arena,
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );

        checker.ctx.symbol_types.insert(symbol, TypeId::ANY);
        assert_eq!(
            FlowAnalyzer::from_ctx(&checker.ctx).fallback_cached_stable_symbol_type(symbol),
            Some(TypeId::ANY),
            "resolved semantic any remains valid generic inference evidence"
        );

        checker.ctx.symbol_resolution_set.insert(symbol);
        assert_eq!(
            FlowAnalyzer::from_ctx(&checker.ctx).fallback_cached_stable_symbol_type(symbol),
            None,
            "an in-flight any sentinel must not become current-pass flow evidence"
        );
    }
}
