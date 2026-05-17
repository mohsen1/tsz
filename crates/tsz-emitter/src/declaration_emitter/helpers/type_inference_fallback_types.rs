//! Fallback expression type lookup helpers for declaration type inference.

use super::super::DeclarationEmitter;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
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

    /// Look up the cached type for a node via its symbol in `symbol_types`.
    /// Unlike `get_type_via_symbol`, this directly queries `symbol_types` without
    /// recursing through declarations — necessary for parameters whose types are
    /// stored by `cache_parameter_types` in `symbol_types` rather than `node_types`.
    pub(crate) fn get_symbol_cached_type(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let cache = self.type_cache.as_ref()?;
        let binder = self.binder?;
        let sym_id = binder.get_node_symbol(node_id)?;
        cache.symbol_types.get(&sym_id).copied()
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
                Some(
                    if self.strict_null_checks {
                        if node.kind == SyntaxKind::NullKeyword as u16 {
                            "null"
                        } else {
                            "undefined"
                        }
                    } else {
                        "any"
                    }
                    .to_string(),
                )
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.preferred_expression_type_text(unary.expression)
                    .or_else(|| self.infer_fallback_type_text_at(unary.expression, depth + 1))
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.function_expression_type_text_from_ast(node_id)
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
                if expr_node.kind == SyntaxKind::Identifier as u16
                    && self.identifier_is_object_rest_binding(expr_idx)
                    && let Some(type_id) = self
                        .get_node_type_or_names(&[expr_idx])
                        .or_else(|| self.get_type_via_symbol(expr_idx))
                    && type_id != tsz_solver::types::TypeId::ANY
                    && type_id != tsz_solver::types::TypeId::ERROR
                    && let Some(interner) = self.type_interner
                    && tsz_solver::type_queries::is_object_like_type(interner, type_id)
                {
                    return Some(self.print_type_id(type_id));
                }
                if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(type_text) =
                        self.property_access_source_accessor_type_text(expr_idx)
                {
                    return Some(type_text);
                }
                if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && self.get_node_type(expr_idx) == Some(tsz_solver::types::TypeId::ANY)
                {
                    return Some("any".to_string());
                }
                let type_text = self
                    .reference_declared_type_annotation_text(expr_idx)
                    .or_else(|| self.value_reference_symbol_type_text(expr_idx))
                    .or_else(|| self.undefined_identifier_type_text(expr_idx));
                if expr_node.kind == SyntaxKind::Identifier as u16
                    && let Some(type_text) = type_text
                {
                    if let Some(excluded_names) = self.object_rest_binding_excluded_names(expr_idx)
                    {
                        return Some(Self::omit_object_type_text_properties(
                            &type_text,
                            &excluded_names,
                        ));
                    }
                    if let Some(type_id) = self.reference_declared_type_id(expr_idx)
                        && self.should_expand_named_application_for_inferred_declaration(type_id)
                    {
                        return Some(self.print_type_id_for_inferred_declaration(type_id));
                    }
                    return Some(type_text);
                }
                type_text
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(type_text) = self.flat_map_array_subclass_return_type_text(expr_idx) {
                    return Some(type_text);
                }
                // Synthesise the source-side intersection text for a
                // generic mixin call like `Mix(A, B)` whose declared
                // return is `T1 & … & Tn` or `T & X`. tsz's inference
                // path can lose one of the type-parameter arms (or
                // expand it structurally), so reading the AST and
                // substituting `typeof argi` in the recognised shape
                // produces the same intersection tsc emits.
                if let Some(text) = self.mixin_call_intersection_source_text(expr_idx) {
                    return Some(text);
                }
                let reused_type_text = self.call_expression_reused_type_text(expr_idx);
                let reused_type_uses_function_local_alias =
                    reused_type_text.as_deref().is_some_and(|type_text| {
                        self.type_text_starts_with_function_local_type_alias(type_text)
                    });
                if reused_type_text.is_some()
                    && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
                    && type_id != tsz_solver::types::TypeId::ANY
                    && type_id != tsz_solver::types::TypeId::ERROR
                    && (reused_type_uses_function_local_alias
                        || self.should_expand_named_application_for_inferred_declaration(type_id)
                        || self.type_contains_conditional_alias_application_for_inferred_emit(
                            type_id, 0,
                        ))
                {
                    let printed = self.print_type_id_for_inferred_declaration(type_id);
                    if let Some(call) = self.arena.get_call_expr(expr_node) {
                        if let Some((alias_name, module_specifier)) =
                            self.call_receiver_default_import_alias(call.expression)
                        {
                            return Some(Self::rewrite_import_type_export_to_default_alias(
                                &printed,
                                &alias_name,
                                &module_specifier,
                            ));
                        }
                    }
                    return Some(printed);
                }
                let reused_type_text = reused_type_text
                    .map(|type_text| {
                        Self::expand_parameters_utility_tuple_type_text(&type_text)
                            .unwrap_or(type_text)
                    })
                    .filter(|type_text| !type_text.is_empty() && type_text != "any");
                reused_type_text.or_else(|| self.call_expression_source_return_type_text(expr_idx))
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                self.tagged_template_declared_return_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.nameable_new_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.conditional_unique_symbol_union_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => self
                .template_index_signature_element_access_type_text(expr_idx)
                .or_else(|| self.class_static_computed_index_access_type_text(expr_idx)),
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                let ast_type_text = self.class_expression_constructor_type_text_from_ast(expr_idx);
                if ast_type_text
                    .as_ref()
                    .is_some_and(|type_text| type_text.contains(" & "))
                    || self
                        .arena
                        .get_class(expr_node)
                        .is_some_and(|class| class.name.is_some())
                {
                    ast_type_text
                } else {
                    self.get_node_type_or_names(&[expr_idx])
                        .map(|type_id| self.print_type_id(type_id))
                        .filter(|type_text| type_text != "any")
                        .or(ast_type_text)
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.array_literal_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.function_expression_type_text_from_ast(expr_idx)
            }
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                self.instantiation_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.short_circuit_expression_type_text(expr_idx)
            }
            _ => None,
        }
    }

    pub(crate) fn json_require_call_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let module_specifier = self.bare_require_call_module_specifier(expr_idx)?;
        if !module_specifier.ends_with(".json") {
            return None;
        }

        let json_path = self.resolve_json_require_path(&module_specifier)?;
        let json_text = std::fs::read_to_string(json_path).ok()?;
        let json_text = Self::strip_json_comments_and_trailing_commas(&json_text);
        let value = serde_json::from_str::<Value>(&json_text).ok()?;
        Some(Self::json_value_declaration_type_text(
            &value,
            self.indent_level,
        ))
    }

    fn resolve_json_require_path(&self, module_specifier: &str) -> Option<PathBuf> {
        let current_path = Path::new(self.current_file_path.as_deref()?);
        let base_dir = current_path.parent()?;
        let candidate = base_dir.join(module_specifier);
        if candidate.is_file() {
            return Some(candidate);
        }
        None
    }

    fn json_value_declaration_type_text(value: &Value, depth: u32) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(_) => "boolean".to_string(),
            Value::Number(_) => "number".to_string(),
            Value::String(_) => "string".to_string(),
            Value::Array(items) => {
                let mut element_types = Vec::new();
                for item in items {
                    let item_type = Self::json_value_declaration_type_text(item, depth);
                    if !element_types.iter().any(|existing| existing == &item_type) {
                        element_types.push(item_type);
                    }
                }
                if element_types.is_empty() {
                    "any[]".to_string()
                } else if element_types.len() == 1 {
                    format!("{}[]", element_types[0])
                } else {
                    format!("({})[]", element_types.join(" | "))
                }
            }
            Value::Object(map) => {
                if map.is_empty() {
                    return "{}".to_string();
                }

                let member_indent = "    ".repeat((depth + 1) as usize);
                let closing_indent = "    ".repeat(depth as usize);
                let mut text = String::from("{\n");
                for (key, value) in map {
                    text.push_str(&member_indent);
                    text.push_str(&Self::json_property_name_text(key));
                    text.push_str(": ");
                    text.push_str(&Self::json_value_declaration_type_text(value, depth + 1));
                    text.push_str(";\n");
                }
                text.push_str(&closing_indent);
                text.push('}');
                text
            }
        }
    }

    fn json_property_name_text(key: &str) -> String {
        let mut chars = key.chars();
        let Some(first) = chars.next() else {
            return "\"\"".to_string();
        };
        let valid_start = first == '_' || first == '$' || first.is_ascii_alphabetic();
        let valid_rest = chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric());
        if valid_start && valid_rest {
            key.to_string()
        } else {
            serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string())
        }
    }

    fn strip_json_comments_and_trailing_commas(text: &str) -> String {
        let mut without_comments = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        let mut in_string = false;
        let mut escaped = false;
        while let Some(ch) = chars.next() {
            if in_string {
                without_comments.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }

            if ch == '"' {
                in_string = true;
                without_comments.push(ch);
                continue;
            }

            if ch == '/' {
                match chars.peek().copied() {
                    Some('/') => {
                        chars.next();
                        for next in chars.by_ref() {
                            if next == '\n' {
                                without_comments.push('\n');
                                break;
                            }
                        }
                        continue;
                    }
                    Some('*') => {
                        chars.next();
                        let mut prev = '\0';
                        for next in chars.by_ref() {
                            if prev == '*' && next == '/' {
                                break;
                            }
                            prev = next;
                        }
                        continue;
                    }
                    _ => {}
                }
            }

            without_comments.push(ch);
        }

        let chars: Vec<char> = without_comments.chars().collect();
        let mut result = String::with_capacity(chars.len());
        let mut index = 0usize;
        in_string = false;
        escaped = false;
        while index < chars.len() {
            let ch = chars[index];
            if in_string {
                result.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                index += 1;
                continue;
            }

            if ch == '"' {
                in_string = true;
                result.push(ch);
                index += 1;
                continue;
            }

            if ch == ',' {
                let mut lookahead = index + 1;
                while lookahead < chars.len() && chars[lookahead].is_whitespace() {
                    lookahead += 1;
                }
                if lookahead < chars.len() && matches!(chars[lookahead], '}' | ']') {
                    index += 1;
                    continue;
                }
            }

            result.push(ch);
            index += 1;
        }
        result
    }

    fn conditional_unique_symbol_union_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let conditional = self.arena.get_conditional_expr(expr_node)?;
        let when_true = self.unique_symbol_reference_typeof_text(conditional.when_true)?;
        let when_false = self.unique_symbol_reference_typeof_text(conditional.when_false)?;
        if when_true == when_false {
            Some(when_true)
        } else {
            Some(format!("{when_true} | {when_false}"))
        }
    }

    fn unique_symbol_reference_typeof_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let name = self.get_identifier_text(expr_idx)?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        if !self.symbol_has_unique_symbol_type(sym_id) {
            return None;
        }
        Some(format!("typeof {name}"))
    }

    fn symbol_has_unique_symbol_type(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));

        if let (Some(cache), Some(interner)) = (self.type_cache.as_ref(), self.type_interner)
            && let Some(type_id) = cache.symbol_types.get(&resolved_sym_id).copied()
            && tsz_solver::type_queries::is_unique_symbol_type(interner, type_id)
        {
            return true;
        }

        let Some(symbol) = binder.symbols.get(resolved_sym_id) else {
            return false;
        };
        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return false;
            };
            let Some(var_decl) = self.arena.get_variable_declaration(decl_node) else {
                return false;
            };
            if var_decl
                .type_annotation
                .into_option()
                .is_some_and(|type_idx| {
                    self.emit_type_node_text(type_idx).as_deref() == Some("unique symbol")
                })
            {
                return true;
            }
            self.arena.is_const_variable_declaration(decl_idx)
                && var_decl.initializer.is_some()
                && self.is_symbol_call(var_decl.initializer)
        })
    }

    pub(in crate::declaration_emitter) fn super_method_call_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let access_node = self.arena.get(call.expression)?;
        if access_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(access_node)?;
        if self
            .arena
            .get(access.expression)
            .is_none_or(|node| node.kind != SyntaxKind::SuperKeyword as u16)
        {
            return None;
        }
        let method_name = self.get_identifier_text(access.name_or_argument)?;
        let is_static_context = self
            .enclosing_method_for_node(expr_idx)
            .is_some_and(|method| self.arena.is_static(&method.modifiers));
        let method_idx =
            self.super_method_declaration(expr_idx, &method_name, is_static_context)?;
        let method_node = self.arena.get(method_idx)?;
        let method = self.arena.get_method_decl(method_node)?;
        self.method_source_return_type_text(method_idx, method)
    }

    fn super_method_declaration(
        &self,
        expr_idx: NodeIndex,
        method_name: &str,
        is_static_context: bool,
    ) -> Option<NodeIndex> {
        let class_idx = self.enclosing_class_for_node(expr_idx)?;
        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;
        let base_expr = self.class_extends_expression(class)?;
        let base_sym = self.value_reference_symbol(base_expr)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(base_sym)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(base_class) = self.arena.get_class(decl_node) else {
                continue;
            };
            if let Some(method_idx) =
                self.class_method_named(base_class, method_name, is_static_context)
            {
                return Some(method_idx);
            }
        }

        None
    }

    fn method_source_return_type_text(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> Option<String> {
        if method.type_annotation.is_some() {
            return self.emit_type_node_text(method.type_annotation);
        }
        if method.body.is_some() {
            if self.body_returns_void(method.body) {
                return Some("void".to_string());
            }
            if let Some(type_text) = self.function_body_preferred_return_type_text(method.body) {
                return Some(type_text);
            }
        }

        let method_type_id = self
            .get_node_type_or_names(&[method_idx, method.name])
            .or_else(|| self.get_type_via_symbol_for_func(method_idx, method.name))?;
        let Some(interner) = self.type_interner else {
            return Some(self.print_type_id(method_type_id));
        };
        tsz_solver::type_queries::get_return_type(interner, method_type_id)
            .map(|return_type| self.print_type_id(return_type))
            .or_else(|| Some(self.print_type_id(method_type_id)))
    }

    fn enclosing_method_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::MethodDeclData> {
        let mut current = node_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if self.arena.get_source_file(parent_node).is_some()
                || self.arena.get_class(parent_node).is_some()
            {
                return None;
            }
            if let Some(method) = self.arena.get_method_decl(parent_node) {
                return Some(method);
            }
            current = parent_idx;
        }
        None
    }

    fn enclosing_class_for_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if self.arena.get_source_file(parent_node).is_some() {
                return None;
            }
            if self.arena.get_class(parent_node).is_some() {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    fn class_extends_expression(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<NodeIndex> {
        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for clause_idx in heritage_clauses.nodes.iter().copied() {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let base_idx = heritage.types.nodes.first().copied()?;
            let base_node = self.arena.get(base_idx)?;
            return self
                .arena
                .get_expr_type_args(base_node)
                .map(|expr| expr.expression)
                .or(Some(base_idx));
        }
        None
    }

    fn class_method_named(
        &self,
        class: &tsz_parser::parser::node::ClassData,
        method_name: &str,
        is_static: bool,
    ) -> Option<NodeIndex> {
        class.members.nodes.iter().copied().find(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            let Some(method) = self.arena.get_method_decl(member_node) else {
                return false;
            };
            self.arena.is_static(&method.modifiers) == is_static
                && self.get_identifier_text(method.name).as_deref() == Some(method_name)
        })
    }
}
