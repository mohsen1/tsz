//! Function return-type normalization helpers for declaration inference.
//!
//! These routines choose source-backed return type text when it carries more
//! declaration information, restore mapped return constraints, and rewrite
//! returned object/class/function text using declaration-scope parameter types.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
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
        if let Some(type_text) =
            self.function_body_numeric_literal_return_union_type_text(&block.statements)
        {
            return Some(type_text);
        }
        if let Some(type_text) =
            self.function_body_string_literal_return_union_type_text(&block.statements)
        {
            return Some(type_text);
        }
        let mut preferred = None;
        if self.collect_unique_return_type_text_from_block(&block.statements, &mut preferred) {
            preferred
        } else {
            None
        }
    }

    pub(in crate::declaration_emitter) fn should_prefer_source_return_type_text(
        &self,
        source_type_text: &str,
        inferred_return_type: tsz_solver::types::TypeId,
    ) -> bool {
        if Self::numeric_literal_union_widens_to_number(
            source_type_text,
            &self.print_type_id(inferred_return_type),
        ) {
            return true;
        }
        if Self::string_literal_union_widens_to_string(
            source_type_text,
            &self.print_type_id(inferred_return_type),
        ) {
            return true;
        }
        if source_type_text.contains("{\n    new ")
            && source_type_text.contains(" & ")
            && self.print_type_id(inferred_return_type) != source_type_text
        {
            return true;
        }
        if Self::type_text_starts_with_import_type(source_type_text)
            && self.print_type_id(inferred_return_type) != source_type_text
        {
            return true;
        }
        if !source_type_text.contains("typeof ") {
            return Self::type_text_contains_mapped_type_literal(source_type_text)
                && self.print_type_id(inferred_return_type) != source_type_text;
        }
        !self.print_type_id(inferred_return_type).contains("typeof ")
    }

    pub(in crate::declaration_emitter) fn source_return_type_is_function_type_param(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> bool {
        let Some(ref type_params) = func.type_parameters else {
            return false;
        };
        let Some(name) = Self::simple_type_reference_name(source_type_text) else {
            return false;
        };
        self.collect_type_param_names(type_params)
            .iter()
            .any(|type_param| type_param == &name)
    }

    pub(in crate::declaration_emitter) fn source_return_type_preserves_function_type_param(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
        inferred_return_type: tsz_solver::types::TypeId,
    ) -> bool {
        let Some(ref type_params) = func.type_parameters else {
            return false;
        };
        let printed = self.print_type_id(inferred_return_type);
        self.collect_type_param_names(type_params)
            .iter()
            .any(|name| {
                Self::contains_whole_word_in_text(source_type_text, name)
                    && !Self::contains_whole_word_in_text(&printed, name)
            })
    }

    pub(in crate::declaration_emitter) fn function_return_type_text_for_declaration_scope(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> (String, bool) {
        let (text, substituted_parameter_type_query) =
            self.substitute_function_parameter_type_queries(func, source_type_text);
        let text = self.rewrite_returned_auto_accessor_parameter_unknowns(func, &text);
        let text = self.rewrite_returned_call_conditional_unknown_subject(func, &text);
        let text = self
            .expand_mapped_alias_index_conditional_text(self.arena, &text)
            .unwrap_or(text);
        let Some(ref type_params) = func.type_parameters else {
            return (text, substituted_parameter_type_query);
        };
        if type_params.nodes.is_empty() {
            return (text, substituted_parameter_type_query);
        }

        let outer_names = self.collect_type_param_names(type_params);
        let text = Self::rename_shadowed_type_params_in_text(&text, &outer_names);
        (
            Self::rename_shadowed_infer_type_params_in_text(&text, &outer_names),
            substituted_parameter_type_query,
        )
    }

    pub(in crate::declaration_emitter) fn inferred_function_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        return_type_id: tsz_solver::types::TypeId,
    ) -> String {
        let text = if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.print_type_id_with_outer_type_params(return_type_id, type_params)
        } else {
            self.print_type_id(return_type_id)
        };
        let text = self.restore_mapped_return_type_param_constraints(func, &text);
        let text = self.rewrite_returned_auto_accessor_parameter_unknowns(func, &text);
        let text = self.rewrite_returned_call_conditional_unknown_subject(func, &text);
        self.expand_mapped_alias_index_conditional_text(self.arena, &text)
            .unwrap_or(text)
    }

    pub(in crate::declaration_emitter) fn restore_mapped_return_type_param_constraints(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        type_text: &str,
    ) -> String {
        if !Self::type_text_contains_mapped_type_literal(type_text) {
            return type_text.to_string();
        }
        let Some(type_params) = func.type_parameters.as_ref() else {
            return type_text.to_string();
        };

        let mut restored = type_text.to_string();
        for &type_param_idx in &type_params.nodes {
            let Some(type_param_node) = self.arena.get(type_param_idx) else {
                continue;
            };
            let Some(type_param) = self.arena.get_type_parameter(type_param_node) else {
                continue;
            };
            let Some(type_param_name) = self.get_identifier_text(type_param.name) else {
                continue;
            };
            if restored.contains(&type_param_name) {
                continue;
            }
            let Some(constraint_text) = self
                .type_annotation_text_from_arena_node(self.arena, type_param.constraint)
                .or_else(|| self.source_slice_from_arena(self.arena, type_param.constraint))
                .map(|text| text.trim().to_string())
            else {
                continue;
            };
            restored = Self::replace_first_mapped_constraint_word(
                &restored,
                &constraint_text,
                &type_param_name,
            );
        }
        Self::ensure_single_line_mapped_member_semicolon(&restored)
    }

    fn replace_first_mapped_constraint_word(type_text: &str, from: &str, to: &str) -> String {
        let Some(mapped_start) = type_text.find(" in ") else {
            return type_text.to_string();
        };
        let search_start = mapped_start + " in ".len();
        let mapped_end = type_text[search_start..]
            .find(']')
            .map(|idx| search_start + idx)
            .unwrap_or(type_text.len());
        let Some(relative_idx) = type_text[search_start..mapped_end].find(from) else {
            return type_text.to_string();
        };
        let start = search_start + relative_idx;
        let end = start + from.len();
        if !Self::whole_word_boundary(type_text, start, end) {
            return type_text.to_string();
        }

        let mut replaced = String::with_capacity(type_text.len() + to.len());
        replaced.push_str(&type_text[..start]);
        replaced.push_str(to);
        replaced.push_str(&type_text[end..]);
        replaced
    }

    fn ensure_single_line_mapped_member_semicolon(type_text: &str) -> String {
        if type_text.contains('\n') || type_text.contains("; }") {
            return type_text.to_string();
        }
        if Self::type_text_contains_mapped_type_literal(type_text)
            && let Some(prefix) = type_text.strip_suffix(" }")
        {
            return format!("{prefix}; }}");
        }
        type_text.to_string()
    }

    fn rewrite_returned_call_conditional_unknown_subject(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> String {
        let Some(rest) = source_type_text.strip_prefix("unknown extends ") else {
            return source_type_text.to_string();
        };
        let fallback_type_param = func
            .type_parameters
            .as_ref()
            .and_then(|type_params| (type_params.nodes.len() == 1).then_some(type_params.nodes[0]))
            .and_then(|type_param_idx| self.arena.get(type_param_idx))
            .and_then(|type_param_node| self.arena.get_type_parameter(type_param_node))
            .and_then(|type_param| self.get_identifier_text(type_param.name));
        let Some(return_arg_idx) = self.single_returned_call_first_argument(func.body) else {
            return fallback_type_param
                .map(|type_param| format!("{type_param} extends {rest}"))
                .unwrap_or_else(|| source_type_text.to_string());
        };
        let Some(return_arg_name) = self.get_identifier_text(return_arg_idx) else {
            return source_type_text.to_string();
        };
        let Some(type_params) = func.type_parameters.as_ref() else {
            return source_type_text.to_string();
        };
        let type_param_names = self.collect_type_param_names(type_params);
        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if self.get_identifier_text(param.name).as_deref() != Some(return_arg_name.as_str()) {
                continue;
            }
            let Some(param_type_text) = self
                .type_annotation_text_from_arena_node(self.arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))
                .map(|text| text.trim().to_string())
            else {
                continue;
            };
            if type_param_names.iter().any(|name| name == &param_type_text) {
                return format!("{param_type_text} extends {rest}");
            }
        }
        fallback_type_param
            .map(|type_param| format!("{type_param} extends {rest}"))
            .unwrap_or_else(|| source_type_text.to_string())
    }

    fn single_returned_call_first_argument(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        if block.statements.nodes.len() != 1 {
            return None;
        }
        let stmt_idx = block.statements.nodes[0];
        let stmt_node = self.arena.get(stmt_idx)?;
        let ret = self.arena.get_return_statement(stmt_node)?;
        let expr_idx = self.skip_parenthesized_expression(ret.expression)?;
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        call.arguments
            .as_ref()
            .and_then(|args| args.nodes.first().copied())
    }

    fn expand_mapped_alias_index_conditional_text(
        &self,
        source_arena: &NodeArena,
        type_text: &str,
    ) -> Option<String> {
        let object_start = type_text.find("{ [")?;
        let object_rest = &type_text[object_start..];
        let object_end = object_rest.find(" ?").map(|idx| object_start + idx)?;
        let mapped_text = object_rest.strip_prefix("{ [")?;
        let in_pos = mapped_text.find(" in keyof ")?;
        let key_param = mapped_text[..in_pos].trim();
        if !Self::is_simple_identifier_text(key_param) {
            return None;
        }
        let alias_start = in_pos + " in keyof ".len();
        let mapped_tail = &mapped_text[alias_start..];
        let alias_application_end = mapped_tail.find("]:").or_else(|| mapped_tail.find('>'))?;
        let alias_application = mapped_tail[..alias_application_end]
            .trim()
            .trim_end_matches(']')
            .trim();
        let (alias_name, alias_arg) = Self::single_type_reference_application(alias_application)?;
        let alias_body =
            self.expand_single_object_type_alias_application(source_arena, alias_name, alias_arg)?;
        Some(format!(
            "{}{alias_body}{}",
            &type_text[..object_start],
            &type_text[object_end..]
        ))
    }

    fn single_type_reference_application(type_text: &str) -> Option<(&str, &str)> {
        let (name, rest) = type_text.split_once('<')?;
        let arg = rest.strip_suffix('>')?;
        let name = name.trim();
        if !Self::is_simple_identifier_text(name) {
            return None;
        }
        Some((name, arg.trim()))
    }

    fn expand_single_object_type_alias_application(
        &self,
        source_arena: &NodeArena,
        alias_name: &str,
        alias_arg: &str,
    ) -> Option<String> {
        let source_file = self.arena_source_file(source_arena)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = source_arena.get(stmt_idx)?;
            let Some(alias) = source_arena.get_type_alias(stmt_node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(source_arena, alias.name)
                .as_deref()
                != Some(alias_name)
            {
                continue;
            }
            let type_params = alias.type_parameters.as_ref()?;
            if type_params.nodes.len() != 1 {
                return None;
            }
            let type_param_node = source_arena.get(type_params.nodes[0])?;
            let type_param = source_arena.get_type_parameter(type_param_node)?;
            let type_param_name = self.identifier_text_from_arena(source_arena, type_param.name)?;
            let alias_text = self
                .source_slice_from_arena(source_arena, alias.type_node)
                .or_else(|| self.emit_type_node_text_from_arena(source_arena, alias.type_node))?;
            let inner = alias_text
                .trim()
                .trim_end_matches(';')
                .trim()
                .strip_prefix('{')?
                .strip_suffix('}')?
                .trim();
            if inner.is_empty() || inner.contains('\n') {
                return None;
            }
            let member = Self::replace_whole_words_in_text(
                inner.trim_end_matches(';').trim(),
                &[(type_param_name, alias_arg.to_string())],
            );
            return Some(format!("{{\n    {member};\n}}"));
        }
        self.expand_single_object_type_alias_application_from_source_text(
            source_arena,
            alias_name,
            alias_arg,
        )
    }

    pub(in crate::declaration_emitter) fn expand_inexact_optional_alias_reference_text(
        &self,
        source_arena: &NodeArena,
        type_text: &str,
    ) -> Option<String> {
        let marker = " & {}";
        let marker_start = type_text.find(marker)?;
        let prefix = type_text.get(..marker_start)?.trim_end();
        let alias_end = prefix.len();
        let alias_start = prefix[..alias_end]
            .char_indices()
            .rev()
            .find_map(|(idx, ch)| {
                (!Self::is_type_reference_identifier_continue(ch)).then_some(idx + ch.len_utf8())
            })
            .unwrap_or(0);
        let alias_name = prefix.get(alias_start..alias_end)?.trim();
        if !Self::is_simple_identifier_text(alias_name) {
            return None;
        }
        let source_object =
            self.inexact_optional_alias_source_object_text(source_arena, alias_name)?;
        let expanded = Self::inexact_optional_object_intersection_text(&source_object)?;
        let mut output = String::with_capacity(type_text.len() - alias_name.len() + expanded.len());
        output.push_str(type_text.get(..alias_start)?);
        output.push_str(&expanded);
        output.push_str(type_text.get(marker_start + marker.len()..)?);
        Some(output)
    }

    fn inexact_optional_alias_source_object_text(
        &self,
        source_arena: &NodeArena,
        alias_name: &str,
    ) -> Option<String> {
        let alias_text = self.source_type_alias_type_text(source_arena, alias_name)?;
        let (mapped_alias_name, mapped_arg) = Self::single_type_reference_application(&alias_text)?;
        if !Self::is_simple_identifier_text(mapped_arg) {
            return None;
        }
        let mapped_alias_text =
            self.source_type_alias_type_text(source_arena, mapped_alias_name)?;
        if !mapped_alias_text.contains("undefined extends")
            || !mapped_alias_text.contains("? K : never")
            || !mapped_alias_text.contains("? never : K")
        {
            return None;
        }
        let source_object = self.source_type_alias_type_text(source_arena, mapped_arg)?;
        Self::leading_balanced_brace_text(&source_object).or(Some(source_object))
    }

    fn source_type_alias_type_text(
        &self,
        source_arena: &NodeArena,
        alias_name: &str,
    ) -> Option<String> {
        let source_file = self.arena_source_file(source_arena)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = source_arena.get(stmt_idx)?;
            let Some(alias) = source_arena.get_type_alias(stmt_node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(source_arena, alias.name)
                .as_deref()
                != Some(alias_name)
            {
                continue;
            }
            return self
                .source_slice_from_arena(source_arena, alias.type_node)
                .or_else(|| self.emit_type_node_text_from_arena(source_arena, alias.type_node))
                .map(|text| text.trim().trim_end_matches(';').trim().to_string());
        }
        None
    }

    fn leading_balanced_brace_text(text: &str) -> Option<String> {
        let trimmed = text.trim_start();
        if !trimmed.starts_with('{') {
            return None;
        }
        let mut depth = 0usize;
        for (idx, ch) in trimmed.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return trimmed.get(..idx + ch.len_utf8()).map(str::to_string);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn expand_single_object_type_alias_application_from_source_text(
        &self,
        source_arena: &NodeArena,
        alias_name: &str,
        alias_arg: &str,
    ) -> Option<String> {
        let source_file = self.arena_source_file(source_arena)?;
        let marker = format!("type {alias_name}<");
        let alias_start = source_file.text.find(&marker)?;
        let param_start = alias_start + marker.len();
        let param_end = source_file.text[param_start..].find('>')? + param_start;
        let type_param_name = source_file.text[param_start..param_end].trim();
        if !Self::is_simple_identifier_text(type_param_name) {
            return None;
        }
        let after_param = &source_file.text[param_end + 1..];
        let equals_pos = after_param.find('=')? + param_end + 1;
        let type_start = equals_pos + 1;
        let type_end = source_file.text[type_start..].find(';')? + type_start;
        let alias_text = source_file.text[type_start..type_end].trim();
        let inner = alias_text.strip_prefix('{')?.strip_suffix('}')?.trim();
        if inner.is_empty() || inner.contains('\n') {
            return None;
        }
        let member = Self::replace_whole_words_in_text(
            inner.trim_end_matches(';').trim(),
            &[(type_param_name.to_string(), alias_arg.to_string())],
        );
        Some(format!("{{\n    {member};\n}}"))
    }

    fn whole_word_boundary(type_text: &str, start: usize, end: usize) -> bool {
        let before = type_text[..start].chars().next_back();
        let after = type_text[end..].chars().next();
        !before.is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            && !after.is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(in crate::declaration_emitter) fn rewrite_returned_auto_accessor_parameter_unknowns(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> String {
        let source_type_text =
            self.restore_mapped_return_type_param_constraints(func, source_type_text);
        let source_type_text = self
            .simplify_uniform_object_keyof_index_access_text(&source_type_text)
            .unwrap_or(source_type_text);
        let source_type_text =
            self.rewrite_returned_object_parameter_unknowns(func, &source_type_text);
        if !source_type_text.contains(": unknown;") {
            return source_type_text;
        }

        let Some(class_expr_idx) = self.direct_returned_class_expression(func.body) else {
            return source_type_text;
        };
        let Some(class_node) = self.arena.get(class_expr_idx) else {
            return source_type_text;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return source_type_text;
        };

        let mut rewritten = source_type_text;
        for member_idx in class.members.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if !self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
            {
                continue;
            }
            if !prop.initializer.is_some() {
                continue;
            }
            let Some(name_text) = self.get_identifier_text(prop.name) else {
                continue;
            };
            let Some(type_text) = self.function_parameter_type_text(func, prop.initializer) else {
                continue;
            };
            if type_text == "unknown" {
                continue;
            }

            let get_unknown = format!("get {name_text}(): unknown;");
            let get_replacement = format!("get {name_text}(): {type_text};");
            rewritten = rewritten.replace(&get_unknown, &get_replacement);

            let set_unknown = format!("set {name_text}(arg: unknown);");
            let set_replacement = format!("set {name_text}(arg: {type_text});");
            rewritten = rewritten.replace(&set_unknown, &set_replacement);
        }

        rewritten
    }

    fn rewrite_returned_object_parameter_unknowns(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> String {
        if !source_type_text.contains(": unknown;") {
            return source_type_text.to_string();
        }

        let Some(object_expr_idx) = self.direct_returned_object_literal(func.body) else {
            return source_type_text.to_string();
        };
        let Some(object_node) = self.arena.get(object_expr_idx) else {
            return source_type_text.to_string();
        };
        let Some(object) = self.arena.get_literal_expr(object_node) else {
            return source_type_text.to_string();
        };

        let mut rewritten = source_type_text.to_string();
        for member_idx in object.elements.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let initializer =
                if let Some(shorthand) = self.arena.get_shorthand_property(member_node) {
                    shorthand.name
                } else if let Some(prop) = self.arena.get_property_assignment(member_node) {
                    prop.initializer
                } else {
                    continue;
                };
            let Some(member_name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(member_name) = self.object_literal_member_name_text(member_name_idx) else {
                continue;
            };
            let Some(type_text) = self.function_parameter_type_text(func, initializer) else {
                continue;
            };
            if type_text == "unknown" {
                continue;
            }
            let unknown_member = format!("{member_name}: unknown;");
            let replacement = format!("{member_name}: {type_text};");
            rewritten = rewritten.replace(&unknown_member, &replacement);
        }

        rewritten
    }

    fn direct_returned_object_literal(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut returned_object = None;
        for stmt_idx in block.statements.nodes.iter().copied() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() {
                return None;
            }
            let expr_idx = self.skip_parenthesized_expression(ret.expression)?;
            let expr_node = self.arena.get(expr_idx)?;
            if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return None;
            }
            if returned_object.replace(expr_idx).is_some() {
                return None;
            }
        }
        returned_object
    }

    fn substitute_function_parameter_type_queries(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> (String, bool) {
        if !source_type_text.contains("typeof ")
            || !source_type_text.contains(" extends ")
            || !source_type_text.contains('?')
        {
            return (source_type_text.to_string(), false);
        }

        let mut text = source_type_text.to_string();
        let mut replaced_any = false;
        for param_idx in func.parameters.nodes.iter().copied() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_name) = self.get_identifier_text(param.name) else {
                continue;
            };
            let Some(param_type_text) = self.function_parameter_type_text(func, param.name) else {
                continue;
            };
            if Self::simple_type_reference_name(&param_type_text).is_none() {
                continue;
            }
            let (replaced_text, replaced) =
                Self::replace_typeof_identifier(&text, &param_name, &param_type_text);
            text = replaced_text;
            replaced_any |= replaced;
        }
        (text, replaced_any)
    }

    pub(in crate::declaration_emitter) fn replace_typeof_identifier(
        text: &str,
        identifier: &str,
        replacement: &str,
    ) -> (String, bool) {
        let query = format!("typeof {identifier}");
        let bytes = text.as_bytes();
        let query_bytes = query.as_bytes();
        let mut result = String::with_capacity(text.len());
        let mut replaced = false;
        let mut i = 0usize;
        while i < bytes.len() {
            if i + query_bytes.len() <= bytes.len()
                && &bytes[i..i + query_bytes.len()] == query_bytes
                && (i == 0 || !Self::is_ident_char(bytes[i - 1]))
            {
                let after = i + query_bytes.len();
                let after_ok = after == bytes.len()
                    || (!Self::is_ident_char(bytes[after])
                        && bytes[after] != b'.'
                        && bytes[after] != b'<');
                if after_ok {
                    result.push_str(replacement);
                    i = after;
                    replaced = true;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }
        (result, replaced)
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
                let type_text = if !ret.expression.is_some() {
                    // `return;` with no expression contributes `void` to the
                    // function's return type — tsc's inference for a bare
                    // return is equivalent to `return undefined` with
                    // widening to `void`. Matches declFileTypeAnnotationBuiltInType.
                    "void".to_string()
                } else if let Some(text) = self
                    .preferred_expression_type_text(ret.expression)
                    .filter(|text| !text.is_empty() && text != "any")
                {
                    text
                } else if let Some(text) = self
                    .return_expression_identifier(ret.expression)
                    .and_then(|identifier_idx| {
                        self.reference_declared_type_annotation_text(identifier_idx)
                    })
                    .filter(|text| text == "any")
                {
                    text
                } else if let Some(text) = self
                    .local_variable_initializer_type_text(ret.expression)
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
                    if if_data.else_statement.is_none() {
                        let mut ignored = preferred.clone();
                        return self.collect_unique_return_type_text_from_statement(
                            if_data.then_statement,
                            &mut ignored,
                        );
                    }
                    self.collect_unique_return_type_text_from_statement(
                        if_data.then_statement,
                        preferred,
                    ) && self.collect_unique_return_type_text_from_statement(
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
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.arena.get_switch(stmt_node).is_some_and(|switch_data| {
                    self.arena
                        .get(switch_data.case_block)
                        .and_then(|case_block_node| self.arena.get_block(case_block_node))
                        .is_some_and(|block| {
                            self.collect_unique_return_type_text_from_block(
                                &block.statements,
                                preferred,
                            )
                        })
                })
            }
            _ => true,
        }
    }
}
