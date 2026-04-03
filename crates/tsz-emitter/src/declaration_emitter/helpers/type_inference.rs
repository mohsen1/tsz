//! Type inference for expressions, object literals, and enums

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    /// Get the type of a node from the type cache, if available.
    pub(crate) fn get_node_type(&self, node_id: NodeIndex) -> Option<tsz_solver::types::TypeId> {
        if let (Some(cache), _) = (&self.type_cache, &self.type_interner) {
            cache.node_types.get(&node_id.0).copied()
        } else {
            None
        }
    }

    /// Try to find type for a function by looking up both the declaration node and name node.
    /// The binder may map the function declaration node rather than the name identifier,
    /// so we try both.
    pub(crate) fn get_type_via_symbol_for_func(
        &self,
        func_idx: NodeIndex,
        name_node: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let cache = self.type_cache.as_ref()?;
        let binder = self.binder?;
        // Try the name node first, then the function declaration node itself
        let symbol_id = binder
            .get_node_symbol(name_node)
            .or_else(|| binder.get_node_symbol(func_idx))?;
        cache.symbol_types.get(&symbol_id).copied()
    }

    pub(crate) fn get_type_via_symbol(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let binder = self.binder?;
        let symbol_id = binder.get_node_symbol(node_id)?;
        let symbol = binder.symbols.get(symbol_id)?;
        symbol
            .declarations
            .iter()
            .copied()
            .find_map(|decl_idx| self.get_node_type_or_names(&[decl_idx]))
    }

    pub(crate) fn infer_fallback_type_text(&self, node_id: NodeIndex) -> Option<String> {
        self.infer_fallback_type_text_at(node_id, self.indent_level)
    }

    pub(in crate::declaration_emitter) fn infer_fallback_type_text_at(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        if !node_id.is_some() {
            return None;
        }

        let node = self.arena.get(node_id)?;
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => Some("RegExp".to_string()),
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                Some("string".to_string())
            }
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16 =>
            {
                Some("any".to_string())
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.preferred_expression_type_text(node_id)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.infer_object_literal_type_text_at(node_id, depth)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .preferred_expression_type_text(node_id)
                .or_else(|| Some("any[]".to_string())),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self
                .infer_arithmetic_binary_type_text(node_id, depth)
                .or_else(|| {
                    self.get_node_type(node_id)
                        .map(|type_id| self.print_type_id(type_id))
                }),
            _ => self
                .get_node_type(node_id)
                .map(|type_id| self.print_type_id(type_id)),
        }
    }

    /// Infer the type of an arithmetic binary expression for declaration emit.
    /// For numeric operators (`+`, `-`, `*`, `/`, `%`, `**`, bitwise), if both
    /// operands resolve to `number`, the result is `number`.
    /// For `+` specifically, if either operand is `string`, the result is `string`.
    pub(in crate::declaration_emitter) fn infer_arithmetic_binary_type_text(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let node = self.arena.get(node_id)?;
        let binary = self.arena.get_binary_expr(node)?;
        let op = binary.operator_token;

        let is_numeric_op = op == SyntaxKind::MinusToken as u16
            || op == SyntaxKind::AsteriskToken as u16
            || op == SyntaxKind::AsteriskAsteriskToken as u16
            || op == SyntaxKind::SlashToken as u16
            || op == SyntaxKind::PercentToken as u16
            || op == SyntaxKind::LessThanLessThanToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16
            || op == SyntaxKind::AmpersandToken as u16
            || op == SyntaxKind::BarToken as u16
            || op == SyntaxKind::CaretToken as u16;

        let is_plus = op == SyntaxKind::PlusToken as u16;

        if !is_numeric_op && !is_plus {
            return None;
        }

        // Purely numeric operators always produce number
        if is_numeric_op {
            return Some("number".to_string());
        }

        // For `+`, resolve both operands
        let left_type = self.infer_operand_type_text(binary.left, depth + 1)?;
        let right_type = self.infer_operand_type_text(binary.right, depth + 1)?;

        if left_type == "string" || right_type == "string" {
            Some("string".to_string())
        } else if left_type == "number" && right_type == "number" {
            Some("number".to_string())
        } else {
            None
        }
    }

    /// Resolve the primitive type of an operand for arithmetic type inference.
    pub(in crate::declaration_emitter) fn infer_operand_type_text(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        // Try preferred expression first (finds declared types)
        if let Some(text) = self.preferred_expression_type_text(node_id) {
            return Some(text);
        }
        // Then try structural fallback
        self.infer_fallback_type_text_at(node_id, depth)
    }

    pub(crate) fn preferred_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        if let Some(asserted_type_text) = self.explicit_asserted_type_text(expr_idx) {
            return Some(asserted_type_text);
        }

        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                self.reference_declared_type_annotation_text(expr_idx)
                    .or_else(|| self.undefined_identifier_type_text(expr_idx))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.call_expression_declared_return_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                self.tagged_template_declared_return_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.nameable_new_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.array_literal_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.short_circuit_expression_type_text(expr_idx)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn declaration_emittable_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
        printed_type_text: &str,
    ) -> String {
        if type_id == tsz_solver::types::TypeId::ANY
            && let Some(type_text) = self.data_view_new_expression_type_text(initializer)
        {
            return type_text;
        }

        if self.object_literal_prefers_syntax_type_text(initializer)
            && let Some(type_text) =
                self.rewrite_object_literal_computed_member_type_text(initializer, type_id)
        {
            return type_text;
        }

        if let Some(typeof_text) =
            self.typeof_prefix_for_value_entity(initializer, true, Some(type_id))
        {
            return typeof_text;
        }

        if (type_id != tsz_solver::types::TypeId::ANY
            || !self.initializer_is_new_expression(initializer))
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            return type_text;
        }

        printed_type_text.to_string()
    }

    pub(in crate::declaration_emitter) fn explicit_asserted_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                current = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                current = binary.right;
                continue;
            }

            let assertion = self.arena.get_type_assertion(node)?;
            let asserted_type = self.arena.get(assertion.type_node)?;
            if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                return None;
            }
            return self.emit_type_node_text(assertion.type_node);
        }

        None
    }

    pub(in crate::declaration_emitter) fn truncation_candidate_type_node(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if let Some(assertion) = self.arena.get_type_assertion(node) {
                let asserted_type = self.arena.get(assertion.type_node)?;
                if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                    return None;
                }
                return Some(assertion.type_node);
            }

            if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                return None;
            }

            let access = self.arena.get_access_expr(node)?;
            let argument = self.arena.get(access.name_or_argument)?;
            let literal = self.arena.get_literal(argument)?;
            if argument.kind != SyntaxKind::NumericLiteral as u16 || literal.text != "0" {
                return None;
            }

            let array_node = self.arena.get(access.expression)?;
            let literal_expr = self.arena.get_literal_expr(array_node)?;
            if array_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || literal_expr.elements.nodes.len() != 1
            {
                return None;
            }

            current = literal_expr.elements.nodes[0];
        }

        None
    }

    pub(in crate::declaration_emitter) fn truncation_candidate_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        if let Some(type_id) = self.get_node_type_or_names(&[type_node]) {
            let printed = self.print_type_id(type_id);
            if printed != "any" {
                return Some(printed);
            }
        }
        self.emit_type_node_text(type_node)
    }

    pub(in crate::declaration_emitter) fn estimated_truncation_candidate_length(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<usize> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        self.estimate_serialized_type_length(type_node, &FxHashMap::default(), 0)
    }

    pub(in crate::declaration_emitter) fn estimate_serialized_type_length(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<usize> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                let mapped = self.arena.get_mapped_type(node)?;
                let type_param = self.arena.get_type_parameter_at(mapped.type_parameter)?;
                let type_param_name = self.get_identifier_text(type_param.name)?;
                let constraint = if type_param.constraint != NodeIndex::NONE {
                    type_param.constraint
                } else {
                    return None;
                };
                let keys = self.expand_string_literals_from_type_node(
                    constraint,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 4usize;
                for key in keys {
                    let mut next = substitutions.clone();
                    next.insert(type_param_name.clone(), key.clone());
                    let value_len =
                        self.estimate_serialized_type_length(mapped.type_node, &next, depth + 1)?;
                    total = total
                        .saturating_add(self.serialized_property_name_length(&key))
                        .saturating_add(2)
                        .saturating_add(value_len)
                        .saturating_add(2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let expansions = self.expand_string_literals_from_type_node(
                    type_node,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 0usize;
                for (idx, value) in expansions.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(value.len() + 2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut total = 0usize;
                for (idx, child) in composite.types.nodes.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(self.estimate_serialized_type_length(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                match literal_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => {
                        Some(self.arena.get_literal(literal_node)?.text.len() + 2)
                    }
                    _ => None,
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn expand_string_literals_from_type_node(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<Vec<String>> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                if literal_node.kind != SyntaxKind::StringLiteral as u16 {
                    return None;
                }
                Some(vec![self.arena.get_literal(literal_node)?.text.clone()])
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut result = Vec::new();
                for child in &composite.types.nodes {
                    result.extend(self.expand_string_literals_from_type_node(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let template = self.arena.get_template_literal_type(node)?;
                let head = self.arena.get(template.head)?;
                let head_text = self
                    .arena
                    .get_literal(head)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let mut results = vec![head_text];
                for span in &template.template_spans.nodes {
                    let data = self.arena.get_template_span_at(*span)?;
                    let expansions = self.expand_string_literals_from_type_node(
                        data.expression,
                        substitutions,
                        depth + 1,
                    )?;
                    let suffix = self
                        .arena
                        .get(data.literal)
                        .and_then(|literal| self.arena.get_literal(literal))
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    let mut next =
                        Vec::with_capacity(results.len().saturating_mul(expansions.len()));
                    for prefix in &results {
                        for expansion in &expansions {
                            let mut combined = String::with_capacity(
                                prefix.len() + expansion.len() + suffix.len(),
                            );
                            combined.push_str(prefix);
                            combined.push_str(expansion);
                            combined.push_str(&suffix);
                            next.push(combined);
                        }
                    }
                    results = next;
                }
                Some(results)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn find_local_type_alias_type_node(
        &self,
        name: &str,
    ) -> Option<NodeIndex> {
        let binder = self.binder?;
        let symbol = binder
            .file_locals
            .get(name)
            .or_else(|| binder.current_scope.get(name))?;
        let declaration = binder.symbols.get(symbol)?.declarations.first().copied()?;
        let declaration_node = self.arena.get(declaration)?;
        self.arena
            .get_type_alias(declaration_node)
            .map(|alias| alias.type_node)
    }

    pub(in crate::declaration_emitter) fn type_reference_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(name_idx);
        }
        if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.arena.get_qualified_name(name_node)?;
            return self.get_identifier_text(qualified.right);
        }
        None
    }

    pub(in crate::declaration_emitter) fn serialized_property_name_length(
        &self,
        name: &str,
    ) -> usize {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return 2;
        };
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return name.len() + 2;
        }
        if chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()) {
            name.len()
        } else {
            name.len() + 2
        }
    }

    pub(in crate::declaration_emitter) fn skip_parenthesized_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;
        loop {
            let node = self.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return Some(current);
            }
            current = self.arena.get_unary_expr_ex(node)?.expression;
        }
    }

    pub(in crate::declaration_emitter) fn arena_source_file<'arena>(
        &self,
        arena: &'arena tsz_parser::parser::node::NodeArena,
    ) -> Option<&'arena tsz_parser::parser::node::SourceFileData> {
        arena
            .nodes
            .iter()
            .rev()
            .find_map(|node| arena.get_source_file(node))
    }

    pub(in crate::declaration_emitter) fn source_slice_from_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(node_idx)?;
        let source_file = self.arena_source_file(arena)?;
        let text = source_file.text.as_ref();
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        text.get(start..end).map(str::to_string)
    }

    pub(crate) fn rescued_asserts_parameter_type_text(
        &self,
        param_idx: NodeIndex,
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let type_node = self.arena.get(param.type_annotation)?;
        let type_ref = self.arena.get_type_ref(type_node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let type_name = self.arena.get(type_ref.type_name)?;
        let ident = self.arena.get_identifier(type_name)?;
        if ident.escaped_text != "asserts" {
            return None;
        }

        let rescued = self.scan_asserts_parameter_type_text(type_node.pos)?;
        let normalized = rescued.split_whitespace().collect::<Vec<_>>().join(" ");
        (normalized != "asserts").then_some(normalized)
    }

    pub(in crate::declaration_emitter) fn scan_asserts_parameter_type_text(
        &self,
        start: u32,
    ) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let start = usize::try_from(start).ok()?;
        if start >= bytes.len() {
            return None;
        }

        let mut i = start;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;

        while i < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => {
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0
                    {
                        break;
                    }
                    paren_depth = paren_depth.saturating_sub(1);
                }
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b',' | b'=' | b';'
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0 =>
                {
                    break;
                }
                _ => {}
            }
            i += 1;
        }

        let rescued = text.get(start..i)?.trim().to_string();
        (!rescued.is_empty()).then_some(rescued)
    }

    pub(in crate::declaration_emitter) fn undefined_identifier_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        (self.get_identifier_text(expr_idx).as_deref() == Some("undefined"))
            .then(|| "any".to_string())
    }

    pub(in crate::declaration_emitter) fn reference_declared_type_annotation_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            // Variable declarations (var/let/const)
            if let Some(var_decl) = self.arena.get_variable_declaration(decl_node)
                && let Some(type_text) = self
                    .preferred_annotation_name_text(var_decl.type_annotation)
                    .or_else(|| self.emit_type_node_text(var_decl.type_annotation))
            {
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }
            // Property declarations (class members)
            if let Some(prop_decl) = self.arena.get_property_decl(decl_node)
                && let Some(type_text) = self
                    .preferred_annotation_name_text(prop_decl.type_annotation)
                    .or_else(|| self.emit_type_node_text(prop_decl.type_annotation))
            {
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }
            // Parameters (function/method parameters)
            if let Some(param) = self.arena.get_parameter(decl_node)
                && let Some(type_text) = self
                    .preferred_annotation_name_text(param.type_annotation)
                    .or_else(|| self.emit_type_node_text(param.type_annotation))
            {
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn local_type_annotation_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let node = self.arena.get(type_idx)?;
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        let slice = text.get(start..end)?.trim();
        (!slice.is_empty()).then(|| slice.to_string())
    }

    pub(in crate::declaration_emitter) fn preferred_annotation_name_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let raw = self.local_type_annotation_text(type_idx)?;
        Self::simple_type_reference_name(&raw).map(|_| raw)
    }

    pub(in crate::declaration_emitter) fn call_expression_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder.symbol_arenas.get(&sym_id)?;
        let source_file = self.arena_source_file(source_arena.as_ref())?;
        if !source_file.is_declaration_file {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = source_arena.get_function(decl_node) else {
                continue;
            };
            if func.type_annotation.is_none() {
                continue;
            }
            if let Some(type_text) =
                self.source_slice_from_arena(source_arena.as_ref(), func.type_annotation)
            {
                return Some(
                    type_text
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .to_string(),
                );
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn tagged_template_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
            return None;
        }

        let tagged = self.arena.get_tagged_template(expr_node)?;
        let sym_id = self.value_reference_symbol(tagged.tag)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder.symbol_arenas.get(&sym_id)?;
        let source_file = self.arena_source_file(source_arena.as_ref())?;
        if !source_file.is_declaration_file {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = source_arena.get_function(decl_node) else {
                continue;
            };
            if func.type_annotation.is_none() {
                continue;
            }
            if let Some(type_text) =
                self.source_slice_from_arena(source_arena.as_ref(), func.type_annotation)
            {
                return Some(
                    type_text
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .to_string(),
                );
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn nameable_new_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.arena.get_call_expr(expr_node)?;
        let base_text = self.declaration_constructor_expression_text(new_expr.expression)?;
        let type_args = self.type_argument_list_source_text(new_expr.type_arguments.as_ref());
        if type_args.is_empty() {
            Some(base_text)
        } else {
            Some(format!("{base_text}<{}>", type_args.join(", ")))
        }
    }

    pub(in crate::declaration_emitter) fn type_argument_list_source_text(
        &self,
        type_args: Option<&NodeList>,
    ) -> Vec<String> {
        let Some(list) = type_args else {
            return Vec::new();
        };

        list.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, &arg)| {
                let node = self.arena.get(arg)?;
                let mut text = self.get_source_slice_no_semi(node.pos, node.end)?;
                if self.first_type_argument_needs_parentheses(arg, index == 0) {
                    text = format!("({text})");
                }
                Some(text)
            })
            .collect()
    }

    pub(crate) fn first_type_argument_needs_parentheses(
        &self,
        type_arg_idx: NodeIndex,
        is_first: bool,
    ) -> bool {
        if !is_first {
            return false;
        }

        self.arena
            .get(type_arg_idx)
            .and_then(|node| self.arena.get_function_type(node))
            .is_some_and(|func| {
                !func
                    .type_parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
            })
    }

    pub(in crate::declaration_emitter) fn declaration_constructor_expression_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.identifier_constructor_reference_text(expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let lhs = self.declaration_constructor_expression_text(access.expression)?;
                let rhs = self.get_identifier_text(access.name_or_argument)?;
                Some(format!("{lhs}.{rhs}"))
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn identifier_constructor_reference_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let ident = self.get_identifier_text(expr_idx)?;
        let binder = self.binder?;
        let sym_id = self.resolve_identifier_symbol(expr_idx, &ident)?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }
            let import_eq = self.arena.get_import_decl(decl_node)?;
            let target_node = self.arena.get(import_eq.module_specifier)?;
            if target_node.kind == SyntaxKind::StringLiteral as u16 {
                return Some(ident);
            }
            return Some(ident);
        }

        Some(ident)
    }

    pub(in crate::declaration_emitter) fn resolve_identifier_symbol(
        &self,
        expr_idx: NodeIndex,
        ident: &str,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let no_libs: &[Arc<BinderState>] = &[];
        binder
            .get_node_symbol(expr_idx)
            .or_else(|| {
                binder.resolve_name_with_filter(ident, self.arena, expr_idx, no_libs, |_| true)
            })
            .or_else(|| binder.file_locals.get(ident))
    }

    pub(in crate::declaration_emitter) fn array_literal_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let array = self.arena.get_literal_expr(expr_node)?;
        if array.elements.nodes.is_empty() {
            return Some("any[]".to_string());
        }

        let mut element_types = Vec::with_capacity(array.elements.nodes.len());
        for elem_idx in array.elements.nodes.iter().copied() {
            // When strictNullChecks is off, skip null/undefined/void elements
            // so they don't pollute the array element type (tsc widens them away).
            if !self.strict_null_checks {
                if let Some(elem_node) = self.arena.get(elem_idx) {
                    let k = elem_node.kind;
                    if k == SyntaxKind::NullKeyword as u16
                        || k == SyntaxKind::UndefinedKeyword as u16
                    {
                        continue;
                    }
                    // Also skip void expressions (e.g., void 0)
                    if self.is_void_expression(elem_node) {
                        continue;
                    }
                }
                // Skip elements whose inferred type is null/undefined
                if let Some(type_id) = self.get_node_type_or_names(&[elem_idx])
                    && matches!(
                        type_id,
                        tsz_solver::types::TypeId::NULL
                            | tsz_solver::types::TypeId::UNDEFINED
                            | tsz_solver::types::TypeId::VOID
                    )
                {
                    continue;
                }
            }
            let elem_type = self.preferred_expression_type_text(elem_idx).or_else(|| {
                self.get_node_type_or_names(&[elem_idx])
                    .map(|type_id| self.print_type_id(type_id))
            })?;
            element_types.push(elem_type);
        }

        let mut distinct = Vec::new();
        for ty in element_types {
            if !distinct.iter().any(|existing| existing == &ty) {
                distinct.push(ty);
            }
        }

        let elem_text = if distinct.len() == 1 {
            distinct.pop()?
        } else {
            distinct.join(" | ")
        };
        let needs_parens =
            elem_text.contains("=>") || elem_text.contains('|') || elem_text.contains('&');
        if needs_parens {
            Some(format!("({elem_text})[]"))
        } else {
            Some(format!("{elem_text}[]"))
        }
    }

    pub(in crate::declaration_emitter) fn short_circuit_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return None;
        }
        if !self.expression_is_always_truthy_for_decl_emit(binary.left) {
            return None;
        }

        self.preferred_expression_type_text(binary.left)
            .or_else(|| {
                self.get_node_type_or_names(&[binary.left])
                    .map(|type_id| self.print_type_id(type_id))
            })
    }

    pub(in crate::declaration_emitter) fn emit_type_node_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        self.arena.get(type_idx)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                self.arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(self.arena)
        };

        scratch.source_is_declaration_file = self.source_is_declaration_file;
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = self.current_source_file_idx;
        scratch.source_file_text = self.source_file_text.clone();
        scratch.current_file_path = self.current_file_path.clone();
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.emit_type(type_idx);
        Some(scratch.writer.take_output())
    }

    pub(in crate::declaration_emitter) fn expression_is_always_truthy_for_decl_emit(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            k if k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.arena.get_binary_expr(expr_node).is_some_and(|binary| {
                    binary.operator_token == SyntaxKind::BarBarToken as u16
                        && self.expression_is_always_truthy_for_decl_emit(binary.left)
                })
            }
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn function_body_preferred_return_type_text(
        &self,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut preferred = None;
        if self.collect_unique_return_type_text_from_block(&block.statements, &mut preferred) {
            preferred
        } else {
            None
        }
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_type_text_from_block(
        &self,
        statements: &NodeList,
        preferred: &mut Option<String>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_unique_return_type_text_from_statement(stmt_idx, preferred)
        })
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_type_text_from_statement(
        &self,
        stmt_idx: NodeIndex,
        preferred: &mut Option<String>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return false;
                };
                let type_text = if let Some(text) = self
                    .preferred_expression_type_text(ret.expression)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else if let Some(text) = self
                    .infer_fallback_type_text_at(ret.expression, 0)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else {
                    return false;
                };
                if let Some(existing) = preferred.as_ref() {
                    existing == &type_text
                } else {
                    *preferred = Some(type_text);
                    true
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_unique_return_type_text_from_block(&block.statements, preferred)
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.collect_unique_return_type_text_from_statement(
                        if_data.then_statement,
                        preferred,
                    ) && if_data.else_statement.is_some()
                        && self.collect_unique_return_type_text_from_statement(
                            if_data.else_statement,
                            preferred,
                        )
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.collect_unique_return_type_text_from_statement(
                        try_data.try_block,
                        preferred,
                    ) && try_data.catch_clause.is_some()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.catch_clause,
                            preferred,
                        )
                        && try_data.finally_block.is_some()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.finally_block,
                            preferred,
                        )
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.collect_unique_return_type_text_from_statement(catch_data.block, preferred)
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.arena.get_case_clause(stmt_node).is_some_and(|clause| {
                    self.collect_unique_return_type_text_from_block(&clause.statements, preferred)
                })
            }
            _ => true,
        }
    }

    pub(in crate::declaration_emitter) fn infer_object_literal_type_text_at(
        &self,
        object_expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;

        // Pre-scan: collect setter and getter names for accessor pair handling
        let mut setter_names = rustc_hash::FxHashSet::<String>::default();
        let mut getter_names = rustc_hash::FxHashSet::<String>::default();
        for &idx in &object.elements.nodes {
            if let Some(n) = self.arena.get(idx) {
                if n.kind == syntax_kind_ext::SET_ACCESSOR {
                    if let Some(acc) = self.arena.get_accessor(n)
                        && let Some(name) = self.object_literal_member_name_text(acc.name)
                    {
                        setter_names.insert(name);
                    }
                } else if n.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(acc) = self.arena.get_accessor(n)
                    && let Some(name) = self.object_literal_member_name_text(acc.name)
                {
                    getter_names.insert(name);
                }
            }
        }

        let mut members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };

            if let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name,
                depth + 1,
                getter_names.contains(&name),
                setter_names.contains(&name),
            ) {
                members.push(member_text);
            }
        }

        if members.is_empty() {
            Some("{}".to_string())
        } else {
            // Format as multi-line to match tsc's .d.ts output
            let member_indent = "    ".repeat((depth + 1) as usize);
            let closing_indent = "    ".repeat(depth as usize);
            let formatted_members: Vec<String> = members
                .iter()
                .map(|m| format!("{member_indent}{m};"))
                .collect();
            Some(format!(
                "{{\n{}\n{closing_indent}}}",
                formatted_members.join("\n")
            ))
        }
    }

    pub(in crate::declaration_emitter) fn infer_object_member_type_text_named_at(
        &self,
        member_idx: NodeIndex,
        name: &str,
        depth: u32,
        getter_exists: bool,
        setter_exists: bool,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_property_assignment(member_node)?;
                let type_text = self
                    .preferred_object_member_initializer_type_text(data.initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_shorthand_property(member_node)?;
                let type_text = self
                    .preferred_object_member_initializer_type_text(
                        data.object_assignment_initializer,
                        depth,
                    )
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node)?;
                // Infer return type: explicit annotation > body inference > any
                let type_text = self
                    .infer_fallback_type_text_at(data.type_annotation, depth)
                    .or_else(|| self.function_body_preferred_return_type_text(data.body))
                    .unwrap_or_else(|| "any".to_string());
                let readonly = if setter_exists { "" } else { "readonly " };
                Some(format!("{readonly}{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if getter_exists {
                    return None;
                }

                let data = self.arena.get_accessor(member_node)?;
                let type_text = data
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&p_idx| self.arena.get(p_idx))
                    .and_then(|p_node| self.arena.get_parameter(p_node))
                    .and_then(|param| {
                        self.infer_fallback_type_text_at(param.type_annotation, depth)
                    })
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node)?;
                let type_text = if data.parameters.nodes.is_empty() {
                    "readonly ".to_string()
                } else {
                    String::new()
                };
                Some(format!("{type_text}{name}: any"))
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn object_literal_member_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        self.resolved_computed_property_name_text(name_idx)
            .or_else(|| self.infer_property_name_text(name_idx))
    }

    pub(in crate::declaration_emitter) fn resolved_computed_property_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = self.arena.get_computed_property(name_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                let interner = self.type_interner?;
                let type_id = self.get_node_type_or_names(&[expr_idx])?;
                let literal = tsz_solver::visitor::literal_value(interner, type_id)?;
                Some(Self::format_property_name_literal_value(&literal, interner))
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_value(
        literal: &tsz_solver::types::LiteralValue,
        interner: &tsz_solver::TypeInterner,
    ) -> String {
        match literal {
            tsz_solver::types::LiteralValue::String(atom) => {
                Self::format_property_name_literal_text(&interner.resolve_atom(*atom))
            }
            tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", interner.resolve_atom(*atom))
            }
        }
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_text(text: &str) -> String {
        if Self::is_unquoted_property_name(text) {
            text.to_string()
        } else {
            format!("\"{}\"", super::escape_string_for_double_quote(text))
        }
    }

    pub(in crate::declaration_emitter) fn is_unquoted_property_name(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };

        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return false;
        }

        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(in crate::declaration_emitter) fn preferred_object_member_initializer_type_text(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let type_id = self.get_node_type_or_names(&[initializer]);
        if let Some(typeof_text) = self.typeof_prefix_for_value_entity(initializer, true, type_id) {
            return Some(typeof_text);
        }
        if let Some(enum_type_text) = self.enum_member_widened_type_text(initializer) {
            return Some(enum_type_text);
        }
        self.preferred_expression_type_text(initializer)
            .or_else(|| self.infer_fallback_type_text_at(initializer, depth))
    }

    pub(in crate::declaration_emitter) fn enum_member_widened_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let binder = self.binder?;

        let member_sym_id = self.value_reference_symbol(expr_idx)?;
        let member_symbol = binder.symbols.get(member_sym_id)?;
        if !member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }

        let enum_expr = self.skip_parenthesized_non_null_and_comma(access.expression);
        let enum_sym_id = self.value_reference_symbol(enum_expr)?;
        let enum_symbol = binder.symbols.get(enum_sym_id)?;
        if !enum_symbol.has_any_flags(symbol_flags::ENUM)
            || enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return None;
        }

        self.nameable_constructor_expression_text(enum_expr)
    }

    pub(in crate::declaration_emitter) fn infer_property_name_text(
        &self,
        node_id: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(node_id)?;
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(computed.expression);
            let expr_node = self.arena.get(expr_idx)?;
            match expr_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    let quote = self.original_quote_char(expr_node);
                    return Some(format!("{}{}{}", quote, literal.text, quote));
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    return Some(Self::normalize_numeric_literal(literal.text.as_ref()));
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                    let unary = self.arena.get_unary_expr(expr_node)?;
                    let operand_idx = self
                        .arena
                        .skip_parenthesized_and_assertions_and_comma(unary.operand);
                    let operand_node = self.arena.get(operand_idx)?;
                    if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                        return None;
                    }
                    let literal = self.arena.get_literal(operand_node)?;
                    let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                    return match unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some(format!("[-{normalized}]")),
                        k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                        _ => None,
                    };
                }
                k if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
                {
                    // Use the COMPUTED_PROPERTY_NAME node's source slice.
                    // The node.end may extend past `]` into trailing `:`
                    // (property colon) or `(` (getter/method params), so
                    // trim to the closing `]` to avoid `::` or `(` leaking.
                    if let Some(mut s) = self.get_source_slice(node.pos, node.end) {
                        // Find the last `]` and truncate after it
                        if let Some(bracket_pos) = s.rfind(']') {
                            s.truncate(bracket_pos + 1);
                        } else {
                            // No brackets — trim trailing punctuation
                            while s.ends_with(':') || s.ends_with('(') {
                                s.pop();
                                s = s.trim_end().to_string();
                            }
                        }
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                    return None;
                }
                _ => return None,
            }
        }
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(literal) = self.arena.get_literal(node) {
            let quote = self.original_quote_char(node);
            return Some(format!("{}{}{}", quote, literal.text, quote));
        }
        self.get_source_slice(node.pos, node.end)
    }

    pub(in crate::declaration_emitter) fn skip_parenthesized_non_null_and_comma(
        &self,
        mut idx: NodeIndex,
    ) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                idx = binary.right;
                continue;
            }
            return idx;
        }
        idx
    }

    pub(in crate::declaration_emitter) fn semantic_simple_enum_access(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if !self.is_simple_enum_access(expr_node) {
            return None;
        }

        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;

        if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
            && symbol.flags & tsz_binder::symbol_flags::ENUM != 0
            && symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0
        {
            return Some(expr_idx);
        }

        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                continue;
            }
            if let Some(enum_data) = self.arena.get_enum(stmt_node)
                && self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
            {
                return Some(expr_idx);
            }
        }
        None
    }

    pub(crate) fn simple_enum_access_member_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let member_name = self.get_identifier_text(access.name_or_argument)?;
            return Some(format!("{base_name}.{member_name}"));
        }

        if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let member_node = self.arena.get(access.name_or_argument)?;
            let member_text = self.get_source_slice(member_node.pos, member_node.end)?;
            return Some(format!("{base_name}[{member_text}]"));
        }

        None
    }

    pub(crate) fn simple_enum_access_base_name_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_node = self.arena.get(access.expression)?;
        self.get_source_slice(base_node.pos, base_node.end)
    }

    pub(crate) fn const_asserted_enum_access_member_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let assertion = self.arena.get_type_assertion(expr_node)?;
        let type_node = self.arena.get(assertion.type_node)?;
        let type_text = self.get_source_slice(type_node.pos, type_node.end)?;
        if type_text != "const" {
            return None;
        }

        self.simple_enum_access_member_text(assertion.expression)
    }

    pub(in crate::declaration_emitter) fn invalid_const_enum_object_access(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_name) = self.get_identifier_text(access.expression) else {
            return false;
        };

        let is_const_enum = if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
        {
            symbol.flags & tsz_binder::symbol_flags::CONST_ENUM != 0
        } else if let Some(source_file_idx) = self.current_source_file_idx
            && let Some(source_file_node) = self.arena.get(source_file_idx)
            && let Some(source_file) = self.arena.get_source_file(source_file_node)
        {
            source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    let Some(stmt_node) = self.arena.get(stmt_idx) else {
                        return false;
                    };
                    if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                        return false;
                    }
                    let Some(enum_data) = self.arena.get_enum(stmt_node) else {
                        return false;
                    };
                    self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
                        && self
                            .arena
                            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
                })
        } else {
            false
        };
        if !is_const_enum {
            return false;
        }

        let argument_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.name_or_argument);
        self.arena
            .get(argument_idx)
            .is_some_and(|arg| arg.kind != SyntaxKind::StringLiteral as u16)
    }

    pub(in crate::declaration_emitter) fn object_literal_prefers_syntax_type_text(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };

        object
            .elements
            .nodes
            .iter()
            .copied()
            .any(|member_idx| self.object_literal_member_needs_syntax_override(member_idx))
    }

    pub(in crate::declaration_emitter) fn rewrite_object_literal_computed_member_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(init_node)?;

        let mut setter_names = rustc_hash::FxHashSet::<String>::default();
        let mut getter_names = rustc_hash::FxHashSet::<String>::default();
        for &idx in &object.elements.nodes {
            if let Some(n) = self.arena.get(idx) {
                if n.kind == syntax_kind_ext::SET_ACCESSOR {
                    if let Some(acc) = self.arena.get_accessor(n)
                        && let Some(name) = self.object_literal_member_name_text(acc.name)
                    {
                        setter_names.insert(name);
                    }
                } else if n.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(acc) = self.arena.get_accessor(n)
                    && let Some(name) = self.object_literal_member_name_text(acc.name)
                {
                    getter_names.insert(name);
                }
            }
        }

        let mut computed_members = Vec::new();
        let mut overridden_members = Vec::new();
        let mut only_numeric_like = true;

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = if let Some(data) = self.arena.get_property_assignment(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_shorthand_property(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_accessor(member_node) {
                Some(data.name)
            } else {
                self.arena
                    .get_method_decl(member_node)
                    .map(|data| data.name)
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if !self.object_literal_member_needs_syntax_override(member_idx) {
                continue;
            }

            let Some(name_text) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };
            let preserve_computed_syntax = name_node.kind
                == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && self
                    .resolved_computed_property_name_text(name_idx)
                    .is_none();
            let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name_text,
                self.indent_level + 1,
                getter_names.contains(&name_text),
                setter_names.contains(&name_text),
            ) else {
                continue;
            };
            if preserve_computed_syntax {
                // Skip methods with computed names — the solver already produces correct
                // method signatures (e.g., `"new"(x: number): number`). Overriding them
                // would emit a wrong property form like `"new": any`.
                if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    continue;
                }
                only_numeric_like &= Self::is_numeric_property_name_text(&name_text);
                computed_members.push((name_text, member_text));
            } else {
                overridden_members.push((name_text, member_text));
            }
        }

        if computed_members.is_empty() && overridden_members.is_empty() {
            return None;
        }

        if overridden_members
            .iter()
            .any(|(_, member_text)| member_text.contains('\n'))
        {
            return self.infer_object_literal_type_text_at(initializer, self.indent_level);
        }

        let printed = self.print_type_id(type_id);
        let mut lines: Vec<String> = printed.lines().map(str::to_string).collect();
        if lines.len() < 2 {
            return Some(printed);
        }

        if only_numeric_like {
            lines.retain(|line| !line.trim_start().starts_with("[x: string]:"));
        }

        let indent = "    ".repeat((self.indent_level + 1) as usize);
        for (name_text, member_text) in overridden_members {
            let replacement = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|line| {
                Self::object_literal_property_line_matches(line, &name_text, &replacement)
            }) {
                lines[existing_idx] = replacement;
            } else {
                let insert_at = lines.len().saturating_sub(1);
                lines.insert(insert_at, replacement);
            }
        }

        let insert_at = lines.len().saturating_sub(1);
        let mut actual_insertions = 0usize;
        for (name_text, member_text) in computed_members {
            let line = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|existing| {
                Self::object_literal_property_line_matches(existing, &name_text, &line)
            }) {
                lines[existing_idx] = line;
            } else {
                let line_trimmed = line.trim();
                if !lines.iter().any(|existing| existing.trim() == line_trimmed) {
                    lines.insert(insert_at + actual_insertions, line);
                    actual_insertions += 1;
                }
            }
        }

        Some(lines.join("\n"))
    }

    pub(in crate::declaration_emitter) fn object_literal_property_line_matches(
        existing: &str,
        name_text: &str,
        replacement: &str,
    ) -> bool {
        let trimmed = existing.trim();
        if trimmed == replacement.trim() {
            return true;
        }

        for prefix in Self::object_literal_property_name_prefixes(name_text) {
            if trimmed.starts_with(&prefix) || trimmed.starts_with(&format!("readonly {prefix}")) {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn object_literal_property_name_prefixes(
        name_text: &str,
    ) -> Vec<String> {
        let mut prefixes = vec![format!("{name_text}:")];

        if let Some(unquoted) = name_text
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .or_else(|| {
                name_text
                    .strip_prefix('\'')
                    .and_then(|name| name.strip_suffix('\''))
            })
        {
            prefixes.push(format!("\"{unquoted}\":"));
            prefixes.push(format!("'{unquoted}':"));
        }

        if let Some(negative_numeric) = name_text
            .strip_prefix("[-")
            .and_then(|name| name.strip_suffix(']'))
        {
            prefixes.push(format!("\"-{negative_numeric}\":"));
            prefixes.push(format!("'-{negative_numeric}':"));
            prefixes.push(format!("-{negative_numeric}:"));
        }

        prefixes
    }

    pub(in crate::declaration_emitter) fn object_literal_member_needs_syntax_override(
        &self,
        member_idx: NodeIndex,
    ) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
            return false;
        };
        if self
            .arena
            .get(name_idx)
            .is_some_and(|name_node| name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        {
            return true;
        }

        let Some(initializer) = self.object_literal_member_initializer(member_node) else {
            return false;
        };
        if self
            .arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && self.object_literal_prefers_syntax_type_text(initializer)
        {
            return true;
        }
        let type_id = self.get_node_type_or_names(&[initializer]);
        self.typeof_prefix_for_value_entity(initializer, true, type_id)
            .is_some()
            || self.enum_member_widened_type_text(initializer).is_some()
    }

    pub(in crate::declaration_emitter) fn object_literal_member_name_idx(
        &self,
        member_node: &Node,
    ) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_shorthand_property(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_accessor(member_node) {
            return Some(data.name);
        }
        self.arena
            .get_method_decl(member_node)
            .map(|data| data.name)
    }

    pub(in crate::declaration_emitter) fn object_literal_member_initializer(
        &self,
        member_node: &Node,
    ) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.initializer);
        }
        self.arena
            .get_shorthand_property(member_node)
            .map(|data| data.object_assignment_initializer)
    }

    pub(in crate::declaration_emitter) fn is_numeric_property_name_text(name: &str) -> bool {
        name.parse::<f64>().is_ok()
            || (name.starts_with("[-")
                && name.ends_with(']')
                && name[2..name.len().saturating_sub(1)].parse::<f64>().is_ok())
    }
}
