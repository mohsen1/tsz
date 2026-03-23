//! Fallback type resolution helpers for assignment flow analysis.
//!
//! These functions derive approximate types from syntax and cached node types
//! when the full checker pipeline hasn't yet resolved a given expression.
//! Used by `get_assigned_type` in `assignment.rs` for flow-sensitive narrowing.

use super::FlowAnalyzer;
use crate::query_boundaries::common::{
    TypeSubstitution, construct_signatures_for_type, instantiate_type,
};
use crate::query_boundaries::flow_analysis::{
    call_signatures_for_type, function_return_type, get_application_info, is_promise_like_type,
    union_members_for_type, unwrap_promise_type_argument, widen_literal_to_primitive,
};
use tsz_common::interner::Atom;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallSignature, CallableShape, PropertyInfo, TypeId, types::ParamInfo};

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

        // Handle binary expressions whose types may only be in request_node_types
        // (contextually typed) rather than node_types. Compute the result type
        // from the operand types which ARE in node_types.
        if rhs_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(rhs_node)
        {
            return self.fallback_binary_expression_type(bin.left, bin.right, bin.operator_token);
        }

        None
    }

    pub(super) fn fallback_expression_type_from_syntax(&self, expr: NodeIndex) -> Option<TypeId> {
        let expr = self.skip_parens_and_assertions(expr);
        if let Some(literal_type) = self.literal_type_from_node(expr) {
            return Some(widen_literal_to_primitive(self.interner, literal_type));
        }
        if let Some(nullish_type) = self.nullish_literal_type(expr) {
            return Some(nullish_type);
        }
        if let Some(ty) = self.node_types.and_then(|nt| nt.get(&expr.0).copied()) {
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

        match node.kind {
            k if k == SyntaxKind::NumberKeyword as u16 => Some(TypeId::NUMBER),
            k if k == SyntaxKind::StringKeyword as u16 => Some(TypeId::STRING),
            k if k == SyntaxKind::BooleanKeyword as u16 => Some(TypeId::BOOLEAN),
            k if k == SyntaxKind::VoidKeyword as u16 => Some(TypeId::VOID),
            k if k == SyntaxKind::AnyKeyword as u16 => Some(TypeId::ANY),
            k if k == SyntaxKind::NeverKeyword as u16 => Some(TypeId::NEVER),
            k if k == SyntaxKind::UnknownKeyword as u16 => Some(TypeId::UNKNOWN),
            k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            k if k == SyntaxKind::ObjectKeyword as u16 => Some(TypeId::OBJECT),
            k if k == SyntaxKind::BigIntKeyword as u16 => Some(TypeId::BIGINT),
            k if k == SyntaxKind::SymbolKeyword as u16 => Some(TypeId::SYMBOL),
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
                    _ => Some(self.interner.union(members)),
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
                    _ => Some(self.interner.intersection(members)),
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                let array = self.arena.get_array_type(node)?;
                let elem = self.fallback_type_from_type_node_syntax(array.element_type)?;
                Some(self.interner.array(elem))
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let sym_id = self
                    .binder
                    .resolve_identifier(self.arena, type_ref.type_name)
                    .or_else(|| self.reference_symbol(type_ref.type_name))?;
                let symbol = self.binder.get_symbol(sym_id)?;
                symbol
                    .declarations
                    .iter()
                    .copied()
                    .find_map(|decl| self.fallback_named_type_declaration_type(decl))
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
        let mut element_types = Vec::new();

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
            _ => self.interner.union(element_types),
        };
        Some(self.interner.array(element_type))
    }

    fn fallback_object_literal_type_from_syntax(&self, expr: NodeIndex) -> Option<TypeId> {
        let node = self.arena.get(expr)?;
        let literal = self.arena.get_literal_expr(node)?;
        let mut properties = Vec::new();

        for &element in &literal.elements.nodes {
            if element.is_none() {
                continue;
            }
            let element_node = self.arena.get(element)?;
            if let Some(prop) = self.arena.get_property_assignment(element_node) {
                let name_atom = self.fallback_object_property_name_atom(prop.name)?;
                let value_type = self.fallback_expression_type_from_syntax(prop.initializer)?;
                properties.push(PropertyInfo::new(name_atom, value_type));
                continue;
            }
            if let Some(shorthand) = self.arena.get_shorthand_property(element_node) {
                let name_node = self.arena.get(shorthand.name)?;
                let ident = self.arena.get_identifier(name_node)?;
                let value_type = self
                    .node_types
                    .and_then(|nt| nt.get(&shorthand.name.0).copied())
                    .or_else(|| self.fallback_type_for_reference(shorthand.name))?;
                properties.push(PropertyInfo::new(ident.atom, value_type));
                continue;
            }
            return None;
        }

        Some(self.interner.factory().object(properties))
    }

    fn fallback_object_property_name_atom(&self, name_idx: NodeIndex) -> Option<Atom> {
        let name_node = self.arena.get(name_idx)?;
        if let Some(ident) = self.arena.get_identifier(name_node) {
            return Some(ident.atom);
        }
        if let Some(literal) = self.arena.get_literal(name_node) {
            return Some(self.interner.intern_string(&literal.text));
        }
        if let Some(computed) = self.arena.get_computed_property(name_node) {
            let key_type = self.fallback_expression_type_from_syntax(computed.expression)?;
            if let Some(literal) = tsz_solver::visitor::literal_value(self.interner, key_type) {
                return Some(match literal {
                    tsz_solver::LiteralValue::Number(value) => {
                        self.interner.intern_string(&value.0.to_string())
                    }
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

    fn fallback_call_expression_type(&self, call_expr: NodeIndex) -> Option<TypeId> {
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
        let mut return_types = Vec::new();
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

        let mut explicit_type_args = Vec::new();
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
                let mut applied_args = explicit_type_args.clone();
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
                if applied_args.len() > sig.type_params.len() {
                    applied_args.truncate(sig.type_params.len());
                }
                let substitution =
                    TypeSubstitution::from_args(self.interner, &sig.type_params, &applied_args);
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
            _ => Some(self.interner.union(awaited_members)),
        }
    }

    fn fallback_type_for_reference(&self, reference: NodeIndex) -> Option<TypeId> {
        let reference = self.skip_parens_and_assertions(reference);
        if let Some(ty) = self.node_types.and_then(|nt| nt.get(&reference.0).copied()) {
            return Some(ty);
        }

        let sym_id = self.reference_symbol(reference)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied()?
        };
        self.fallback_declaration_type(decl)
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
            });
        }

        Some(self.interner.factory().callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: Vec::new(),
                params,
                this_type,
                return_type: self
                    .arena
                    .get(decl)
                    .and_then(|node| self.arena.get_function(node))
                    .and_then(|func| {
                        func.type_annotation.is_some().then_some(func.type_annotation)
                    })
                    .and_then(|type_ann| self.fallback_type_from_type_node_syntax(type_ann))
                    .unwrap_or(TypeId::ANY),
                type_predicate: None,
                is_method: false,
            }],
            construct_signatures: Vec::new(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        }))
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
                        && self.type_annotation_looks_like_promise(func.type_annotation)
                })
    }

    fn declaration_is_async_function(&self, decl: NodeIndex) -> bool {
        self.arena
            .get(decl)
            .and_then(|node| self.arena.get_function(node))
            .is_some_and(|func| func.is_async)
    }

    fn type_annotation_looks_like_promise(&self, type_annotation: NodeIndex) -> bool {
        let Some(node) = self.arena.get(type_annotation) else {
            return false;
        };

        let Some(type_ref) = self.arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(type_ref.type_name) else {
            return false;
        };

        if let Some(ident) = self.arena.get_identifier(name_node) {
            return self.is_promise_like_name(ident.escaped_text.as_str());
        }

        self.arena
            .get_qualified_name(name_node)
            .and_then(|qualified| self.arena.get(qualified.right))
            .and_then(|node| self.arena.get_identifier(node))
            .is_some_and(|ident| self.is_promise_like_name(ident.escaped_text.as_str()))
    }

    fn is_promise_like_name(&self, name: &str) -> bool {
        matches!(name, "Promise" | "PromiseLike") || name.contains("Promise")
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
        if operator == SyntaxKind::QuestionQuestionToken as u16 {
            // x ?? y -> NonNullable<typeof x> | typeof y
            let left_type = self.resolve_operand_type(left)?;
            let right_type = self.resolve_operand_type(right)?;
            let non_nullish_left = self.interner.remove_nullish(left_type);
            return Some(self.interner.union2(non_nullish_left, right_type));
        }
        if operator == SyntaxKind::BarBarToken as u16 {
            // x || y -> NonNullable<typeof x> | typeof y
            // TypeScript narrows the left side in || result types: the truthy branch
            // removes null/undefined (and other falsy types, but removing nullish covers
            // the most important case for flow analysis).
            let left_type = self.resolve_operand_type(left)?;
            let right_type = self.resolve_operand_type(right)?;
            let non_nullish_left = self.interner.remove_nullish(left_type);
            return Some(self.interner.union2(non_nullish_left, right_type));
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

    /// Resolve the type of an expression operand using node_types cache,
    /// literal detection, or reference resolution.
    fn resolve_operand_type(&self, idx: NodeIndex) -> Option<TypeId> {
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
            _ => Some(self.interner.union(types)),
        }
    }
}
