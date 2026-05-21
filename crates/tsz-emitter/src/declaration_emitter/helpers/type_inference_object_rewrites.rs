//! Object-literal computed member and index-signature rewrite helpers.
//!
//! This module owns the declaration emitter cleanup passes that recover source
//! syntax for computed object-literal members after solver type printing has
//! widened them into object/index-signature text.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComputedObjectIndexKeyKind {
    ConcreteString,
    ConcreteNumber,
    ConcreteSymbol,
    DynamicString,
    DynamicNumber,
    DynamicSymbol,
    Unknown,
}

#[derive(Debug)]
struct ComputedObjectIndexMember {
    kind: ComputedObjectIndexKeyKind,
    name_text: Option<String>,
    value_type: String,
}

impl<'a> DeclarationEmitter<'a> {
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
        let mut concrete_member_names = Vec::new();
        let mut only_numeric_like = true;
        let mut has_non_emittable_computed_members = false;
        let mut synthetic_number_index_member = None;
        let mut negative_numeric_computed_names = Vec::new();
        let mut computed_method_value_types = Vec::new();

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

            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(source_name_text) = self.get_source_slice(name_node.pos, name_node.end)
                && let Some(key_text) =
                    Self::negative_numeric_computed_property_key_text(&source_name_text)
            {
                negative_numeric_computed_names
                    .push((key_text.to_string(), source_name_text.trim().to_string()));
            }

            let Some(mut name_text) = self.object_literal_member_name_text(name_idx) else {
                if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    has_non_emittable_computed_members = true;
                    if synthetic_number_index_member.is_none() {
                        synthetic_number_index_member = self
                            .infer_object_member_type_text_named_at(
                                member_idx,
                                "[x: number]",
                                self.indent_level + 1,
                                false,
                                false,
                            );
                    }
                }
                continue;
            };
            let preserve_computed_syntax = name_node.kind
                == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && self
                    .resolved_computed_property_name_text(name_idx)
                    .is_none();
            if preserve_computed_syntax
                && let Some(source_name_text) = self.get_source_slice(name_node.pos, name_node.end)
                && Self::is_negative_numeric_computed_property_name_text(&source_name_text)
            {
                name_text = source_name_text.trim().to_string();
            }
            concrete_member_names.push(name_text.clone());
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
                if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    if let Some(value_type) = Self::object_literal_property_value_type(&member_text)
                    {
                        computed_method_value_types.push(value_type.to_string());
                    }
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
        let recovered_computed_index_signatures =
            self.rewrite_object_literal_computed_index_signatures(initializer, &mut lines);
        if !recovered_computed_index_signatures
            && let Some(source_union) =
                self.source_ordered_object_literal_index_value_union_text(initializer)
        {
            Self::rewrite_broad_index_signature_value_union(&mut lines, &source_union);
        }

        if has_non_emittable_computed_members {
            let index_signature_value_types: Vec<String> = lines
                .iter()
                .filter_map(|line| {
                    Self::broad_object_index_signature_value_type(line).map(str::to_string)
                })
                .collect();
            if !index_signature_value_types.is_empty() {
                computed_members.retain(|(_, member_text)| {
                    Self::object_literal_property_value_type(member_text).is_none_or(
                        |member_value_type| {
                            !index_signature_value_types
                                .iter()
                                .any(|index_value_type| index_value_type == member_value_type)
                        },
                    )
                });
            }
        } else {
            let computed_value_types: Vec<String> = computed_members
                .iter()
                .filter_map(|(name_text, member_text)| {
                    Self::is_symbol_observer_computed_property_name_text(name_text)
                        .then(|| Self::object_literal_property_value_type(member_text))
                        .flatten()
                        .map(str::to_string)
                })
                .collect();
            if !computed_value_types.is_empty() {
                lines.retain(|line| {
                    let Some(index_value_type) =
                        Self::broad_object_index_signature_value_type(line)
                    else {
                        return true;
                    };
                    !computed_value_types
                        .iter()
                        .any(|member_value_type| member_value_type == index_value_type)
                });
            }
            if !computed_method_value_types.is_empty() {
                lines.retain(|line| {
                    let Some(index_value_type) =
                        Self::broad_object_index_signature_value_type(line)
                    else {
                        return true;
                    };
                    !computed_method_value_types
                        .iter()
                        .any(|member_value_type| member_value_type == index_value_type)
                });
            }
        }

        if only_numeric_like {
            if has_non_emittable_computed_members {
                for line in &mut lines {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("[x: string]:") {
                        *line = line.replacen("[x: string]:", "[x: number]:", 1);
                    } else if trimmed.starts_with("readonly [x: string]:") {
                        *line = line.replacen("readonly [x: string]:", "readonly [x: number]:", 1);
                    }
                }
                lines.retain(|line| {
                    let trimmed = line.trim_start();
                    if Self::is_numeric_like_object_property_line(trimmed)
                        && !Self::object_literal_line_matches_any_name(
                            trimmed,
                            &concrete_member_names,
                        )
                    {
                        return false;
                    }
                    true
                });
                let has_number_index = lines
                    .iter()
                    .any(|line| line.trim_start().starts_with("[x: number]:"));
                if !has_number_index
                    && let Some(member_text) = synthetic_number_index_member.as_deref()
                {
                    let member_indent = "    ".repeat((self.indent_level + 1) as usize);
                    lines.insert(1, format!("{member_indent}{member_text};"));
                }
            } else {
                lines.retain(|line| !line.trim_start().starts_with("[x: string]:"));
            }
        }

        let indent = "    ".repeat((self.indent_level + 1) as usize);
        for (name_text, member_text) in overridden_members {
            let replacement = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|line| {
                Self::object_literal_property_line_matches(line, &name_text, &replacement)
            }) {
                let existing_end_idx = Self::object_literal_member_line_end(&lines, existing_idx);
                lines[existing_idx] = replacement;
                if existing_end_idx > existing_idx {
                    lines.drain(existing_idx + 1..=existing_end_idx);
                }
            } else if lines.iter().any(|existing| {
                Self::object_literal_method_line_matches_property_function(
                    existing,
                    &name_text,
                    &replacement,
                )
            }) {
                continue;
            } else {
                let insert_at = lines.len().saturating_sub(1);
                lines.insert(insert_at, replacement);
            }
        }

        let insert_at = lines.len().saturating_sub(1);
        let mut actual_insertions = 0usize;
        for (name_text, member_text) in computed_members {
            let line = format!("{indent}{member_text};");
            let line_trimmed = line.trim();
            if Self::object_literal_method_line_matches_name(line_trimmed, &name_text)
                && lines.iter().any(|existing| {
                    Self::object_literal_method_line_matches_name(existing.trim(), &name_text)
                })
            {
                continue;
            }
            let exact_exists = lines.iter().any(|existing| existing.trim() == line_trimmed);
            if let Some(existing_idx) = lines.iter().position(|existing| {
                existing.trim() != line_trimmed
                    && Self::object_literal_property_line_matches(existing, &name_text, &line)
            }) {
                if exact_exists {
                    lines.remove(existing_idx);
                } else {
                    let existing_end_idx =
                        Self::object_literal_member_line_end(&lines, existing_idx);
                    lines[existing_idx] = line;
                    if existing_end_idx > existing_idx {
                        lines.drain(existing_idx + 1..=existing_end_idx);
                    }
                }
            } else if lines.iter().any(|existing| {
                Self::object_literal_method_line_matches_property_function(
                    existing, &name_text, &line,
                )
            }) {
                continue;
            } else if !exact_exists {
                lines.insert(insert_at + actual_insertions, line);
                actual_insertions += 1;
            }
        }

        if !negative_numeric_computed_names.is_empty() {
            for line in &mut lines {
                for (key_text, source_name_text) in &negative_numeric_computed_names {
                    if Self::replace_object_literal_property_line_name(
                        line,
                        key_text,
                        source_name_text,
                    ) {
                        break;
                    }
                }
            }
        }

        self.remove_dynamic_computed_object_literal_property_lines(initializer, &mut lines);
        self.deduplicate_object_literal_property_lines(&mut lines);

        Some(lines.join("\n"))
    }

    fn rewrite_object_literal_computed_index_signatures(
        &self,
        object_expr_idx: NodeIndex,
        lines: &mut Vec<String>,
    ) -> bool {
        let members = self.computed_object_index_members(object_expr_idx);
        if members.is_empty() {
            return false;
        }

        let has_dynamic_string = members
            .iter()
            .any(|member| member.kind == ComputedObjectIndexKeyKind::DynamicString);
        let has_dynamic_number = members
            .iter()
            .any(|member| member.kind == ComputedObjectIndexKeyKind::DynamicNumber);
        let has_dynamic_symbol = members
            .iter()
            .any(|member| member.kind == ComputedObjectIndexKeyKind::DynamicSymbol);

        if !has_dynamic_string && !has_dynamic_number && !has_dynamic_symbol {
            self.deduplicate_object_literal_property_lines(lines);
            return false;
        }

        let mut concrete_string_or_number = Vec::new();
        let mut dynamic_string_or_number = Vec::new();
        let mut number_values = Vec::new();
        let mut symbol_values = Vec::new();
        let mut dynamic_names = Vec::new();

        for member in &members {
            match member.kind {
                ComputedObjectIndexKeyKind::ConcreteString => {
                    Self::push_unique_type_text(&mut concrete_string_or_number, &member.value_type);
                }
                ComputedObjectIndexKeyKind::ConcreteNumber => {
                    Self::push_unique_type_text(&mut concrete_string_or_number, &member.value_type);
                    Self::push_unique_type_text(&mut number_values, &member.value_type);
                }
                ComputedObjectIndexKeyKind::ConcreteSymbol => {
                    Self::push_unique_type_text(&mut symbol_values, &member.value_type);
                }
                ComputedObjectIndexKeyKind::DynamicString => {
                    Self::push_unique_type_text(&mut dynamic_string_or_number, &member.value_type);
                    if let Some(name_text) = member.name_text.as_deref() {
                        dynamic_names.push(name_text.to_string());
                    }
                }
                ComputedObjectIndexKeyKind::DynamicNumber => {
                    Self::push_unique_type_text(&mut dynamic_string_or_number, &member.value_type);
                    Self::push_unique_type_text(&mut number_values, &member.value_type);
                    if let Some(name_text) = member.name_text.as_deref() {
                        dynamic_names.push(name_text.to_string());
                    }
                }
                ComputedObjectIndexKeyKind::DynamicSymbol => {
                    Self::push_unique_type_text(&mut symbol_values, &member.value_type);
                    if let Some(name_text) = member.name_text.as_deref() {
                        dynamic_names.push(name_text.to_string());
                    }
                }
                ComputedObjectIndexKeyKind::Unknown => {}
            }
        }

        lines.retain(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("[x: string]:")
                && !trimmed.starts_with("[x: number]:")
                && !trimmed.starts_with("[x: symbol]:")
                && !dynamic_names
                    .iter()
                    .any(|name| Self::object_literal_property_line_matches(line, name, ""))
        });

        let mut new_index_lines = Vec::new();
        let indent = "    ".repeat((self.indent_level + 1) as usize);
        if has_dynamic_string {
            let mut values = concrete_string_or_number.clone();
            for value in &dynamic_string_or_number {
                Self::push_unique_type_text(&mut values, value);
            }
            if !values.is_empty() {
                new_index_lines.push(format!("{indent}[x: string]: {};", values.join(" | ")));
            }
        }
        if has_dynamic_number && !number_values.is_empty() {
            new_index_lines.push(format!(
                "{indent}[x: number]: {};",
                number_values.join(" | ")
            ));
        }
        if has_dynamic_symbol && !symbol_values.is_empty() {
            new_index_lines.push(format!(
                "{indent}[x: symbol]: {};",
                symbol_values.join(" | ")
            ));
        }

        for (offset, line) in new_index_lines.into_iter().enumerate() {
            lines.insert(1 + offset, line);
        }
        self.deduplicate_object_literal_property_lines(lines);
        true
    }

    fn computed_object_index_members(
        &self,
        object_expr_idx: NodeIndex,
    ) -> Vec<ComputedObjectIndexMember> {
        let Some(object_node) = self.arena.get(object_expr_idx) else {
            return Vec::new();
        };
        let Some(object) = self.arena.get_literal_expr(object_node) else {
            return Vec::new();
        };
        let mut members = Vec::new();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }

            let Some(value_type) = self.object_literal_member_index_value_type_text(member_idx)
            else {
                continue;
            };
            let name_text = self
                .object_literal_member_name_text(name_idx)
                .or_else(|| self.constant_computed_property_name_text(name_idx));
            let kind = self.computed_object_index_key_kind(name_idx, name_text.as_deref());
            members.push(ComputedObjectIndexMember {
                kind,
                name_text,
                value_type: Self::parenthesize_type_text_in_union_position(&value_type),
            });
        }

        members
    }

    fn constant_computed_property_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        let computed = self.arena.get_computed_property(name_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        self.constant_computed_key_expression_text(expr_idx)
    }

    fn constant_computed_key_expression_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::NumericLiteral as u16 {
            let value = self.arena.get_literal(expr_node)?.value?;
            return Some(Self::format_js_number(value));
        }
        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let binary = self.arena.get_binary_expr(expr_node)?;
            if binary.operator_token != SyntaxKind::PlusToken as u16 {
                return None;
            }
            let left = self.constant_computed_key_number_value(binary.left)?;
            let right = self.constant_computed_key_number_value(binary.right)?;
            return Some(Self::format_js_number(left + right));
        }
        None
    }

    fn constant_computed_key_number_value(&self, expr_idx: NodeIndex) -> Option<f64> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::NumericLiteral as u16 {
            return self.arena.get_literal(expr_node)?.value;
        }
        None
    }

    fn remove_dynamic_computed_object_literal_property_lines(
        &self,
        object_expr_idx: NodeIndex,
        lines: &mut Vec<String>,
    ) {
        let dynamic_names: Vec<String> = self
            .computed_object_index_members(object_expr_idx)
            .into_iter()
            .filter_map(|member| {
                matches!(
                    member.kind,
                    ComputedObjectIndexKeyKind::DynamicString
                        | ComputedObjectIndexKeyKind::DynamicNumber
                        | ComputedObjectIndexKeyKind::DynamicSymbol
                )
                .then_some(member.name_text)
                .flatten()
            })
            .collect();

        if dynamic_names.is_empty() {
            return;
        }

        lines.retain(|line| {
            !dynamic_names
                .iter()
                .any(|name| Self::object_literal_property_line_matches(line, name, ""))
        });
    }

    fn computed_object_index_key_kind(
        &self,
        name_idx: NodeIndex,
        name_text: Option<&str>,
    ) -> ComputedObjectIndexKeyKind {
        let Some(name_node) = self.arena.get(name_idx) else {
            return ComputedObjectIndexKeyKind::Unknown;
        };
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return ComputedObjectIndexKeyKind::Unknown;
        };
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return ComputedObjectIndexKeyKind::Unknown;
        };

        match expr_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                return ComputedObjectIndexKeyKind::ConcreteString;
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                return ComputedObjectIndexKeyKind::ConcreteNumber;
            }
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                if let Some(type_id) = self.get_node_type_or_names(&[expr_idx]) {
                    let is_unique_symbol = self.type_interner.is_some_and(|interner| {
                        tsz_solver::visitor::unique_symbol_ref(interner, type_id).is_some()
                    });
                    if type_id == tsz_solver::TypeId::SYMBOL || is_unique_symbol {
                        return ComputedObjectIndexKeyKind::ConcreteSymbol;
                    }
                }
                if name_text.is_some() {
                    return ComputedObjectIndexKeyKind::ConcreteString;
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if self.is_symbol_call(expr_idx) {
                    return ComputedObjectIndexKeyKind::DynamicSymbol;
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if self.computed_binary_expression_is_number_like(expr_idx) {
                    return ComputedObjectIndexKeyKind::DynamicNumber;
                }
                return ComputedObjectIndexKeyKind::DynamicString;
            }
            _ => {}
        }

        ComputedObjectIndexKeyKind::Unknown
    }

    fn computed_binary_expression_is_number_like(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(binary) = self.arena.get_binary_expr(expr_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::PlusToken as u16 {
            return false;
        }
        self.computed_key_expression_is_number_like(binary.left)
            && self.computed_key_expression_is_number_like(binary.right)
    }

    fn computed_key_expression_is_number_like(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        expr_node.kind == SyntaxKind::NumericLiteral as u16
            || self
                .get_node_type_or_names(&[expr_idx])
                .is_some_and(|type_id| type_id == tsz_solver::TypeId::NUMBER)
    }

    fn push_unique_type_text(values: &mut Vec<String>, value: &str) {
        if !values.iter().any(|existing| existing == value) {
            values.push(value.to_string());
        }
    }

    fn deduplicate_object_literal_property_lines(&self, lines: &mut Vec<String>) {
        let mut seen = Vec::<String>::new();
        lines.retain(|line| {
            let Some(key) = Self::object_literal_property_identity_key(line) else {
                return true;
            };
            if seen.iter().any(|existing| existing == &key) {
                return false;
            }
            seen.push(key);
            true
        });
    }

    fn object_literal_property_identity_key(line: &str) -> Option<String> {
        let trimmed = line.trim().trim_end_matches(';').trim();
        if trimmed.starts_with("[x: ") || trimmed == "{" || trimmed == "}" {
            return None;
        }
        let colon_idx = if trimmed.starts_with('[') {
            let bracket_end = trimmed.find(']')?;
            trimmed.get(bracket_end + 1..)?.find(':')? + bracket_end + 1
        } else {
            trimmed.find(':')?
        };
        let name = trimmed[..colon_idx]
            .trim()
            .strip_prefix("readonly ")
            .unwrap_or(trimmed[..colon_idx].trim())
            .trim();
        let value = trimmed[colon_idx + 1..].trim();
        let normalized_name = name
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .or_else(|| {
                name.strip_prefix('\'')
                    .and_then(|name| name.strip_suffix('\''))
            })
            .unwrap_or(name);
        Some(format!("{normalized_name}:{value}"))
    }

    pub(in crate::declaration_emitter) fn source_ordered_object_literal_index_value_union_text(
        &self,
        object_expr_idx: NodeIndex,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;
        let mut value_types = Vec::new();

        for &member_idx in &object.elements.nodes {
            let member_type = self.object_literal_member_index_value_type_text(member_idx)?;
            let member_type = Self::parenthesize_type_text_in_union_position(&member_type);
            if !value_types.iter().any(|existing| existing == &member_type) {
                value_types.push(member_type);
            }
        }

        (value_types.len() > 1).then(|| value_types.join(" | "))
    }

    fn object_literal_member_index_value_type_text(&self, member_idx: NodeIndex) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        if member_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
            return None;
        }
        if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
            let method = self.arena.get_method_decl(member_node)?;
            return self.method_function_type_text(member_idx, method, self.indent_level + 1);
        }

        let initializer = self.object_literal_member_initializer(member_node)?;
        self.const_asserted_expression_type_text(initializer, self.indent_level + 1)
            .or_else(|| self.preferred_expression_type_text(initializer))
            .or_else(|| {
                self.get_node_type_or_names(&[initializer])
                    .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            })
            .or_else(|| self.infer_fallback_type_text_at(initializer, self.indent_level + 1))
    }

    pub(super) fn parenthesize_type_text_in_union_position(type_text: &str) -> String {
        let trimmed = type_text.trim();
        if (trimmed.contains("=>") || trimmed.starts_with("new "))
            && !(trimmed.starts_with('(') && trimmed.ends_with(')'))
        {
            format!("({trimmed})")
        } else {
            trimmed.to_string()
        }
    }

    pub(in crate::declaration_emitter) fn rewrite_broad_index_signature_value_union(
        lines: &mut [String],
        source_union: &str,
    ) {
        let source_parts = Self::split_top_level_union_type_parts(source_union);
        if source_parts.len() <= 1 {
            return;
        }

        for line in lines {
            let Some(existing_union) = Self::broad_object_index_signature_value_type(line) else {
                continue;
            };
            let existing_parts = Self::split_top_level_union_type_parts(existing_union);
            if existing_parts.len() != source_parts.len() {
                continue;
            }
            if !source_parts
                .iter()
                .all(|source| existing_parts.iter().any(|existing| existing == source))
            {
                continue;
            }
            *line = line.replacen(existing_union, source_union, 1);
        }
    }

    pub(super) fn split_top_level_union_type_parts(type_text: &str) -> Vec<String> {
        let bytes = type_text.as_bytes();
        let mut parts = Vec::new();
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut part_start = 0usize;
        let mut i = 0usize;

        while i < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' if i == 0 || bytes[i - 1] != b'=' => {
                    angle_depth = angle_depth.saturating_sub(1);
                }
                b'|' if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    if let Some(part) = type_text.get(part_start..i) {
                        let trimmed = part.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                        }
                    }
                    part_start = i + 1;
                }
                _ => {}
            }
            i += 1;
        }

        if let Some(part) = type_text.get(part_start..) {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
        parts
    }

    fn object_literal_member_line_end(lines: &[String], start_idx: usize) -> usize {
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut escaped = false;

        for (idx, line) in lines.iter().enumerate().skip(start_idx) {
            for byte in line.bytes() {
                if let Some(active_quote) = quote {
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == active_quote {
                        quote = None;
                    }
                    continue;
                }

                match byte {
                    b'\'' | b'"' | b'`' => quote = Some(byte),
                    b'(' => paren_depth += 1,
                    b')' => paren_depth = paren_depth.saturating_sub(1),
                    b'[' => bracket_depth += 1,
                    b']' => bracket_depth = bracket_depth.saturating_sub(1),
                    b'{' => brace_depth += 1,
                    b'}' => brace_depth = brace_depth.saturating_sub(1),
                    b'<' => angle_depth += 1,
                    b'>' if angle_depth > 0 => angle_depth -= 1,
                    _ => {}
                }
            }

            if quote.is_none()
                && paren_depth == 0
                && bracket_depth == 0
                && brace_depth == 0
                && angle_depth == 0
                && line.trim_end().ends_with(';')
            {
                return idx;
            }
        }

        start_idx
    }

    pub(in crate::declaration_emitter) fn object_literal_method_line_matches_name(
        existing: &str,
        name_text: &str,
    ) -> bool {
        let without_readonly = existing
            .strip_prefix("readonly ")
            .unwrap_or(existing)
            .trim_start();
        without_readonly.starts_with(&format!("{name_text}("))
            || without_readonly.starts_with(&format!("{name_text}<"))
            || without_readonly.starts_with(&format!("{name_text}?("))
            || without_readonly.starts_with(&format!("{name_text}?<"))
    }

    fn object_literal_method_line_matches_property_function(
        existing: &str,
        name_text: &str,
        property_function_line: &str,
    ) -> bool {
        let Some(property_value_type) =
            Self::object_literal_property_value_type(property_function_line)
        else {
            return false;
        };
        if !property_value_type.contains("=>") {
            return false;
        }

        let existing = existing.trim().trim_end_matches(';').trim();
        for prefix in Self::object_literal_method_name_prefixes(name_text) {
            let Some(after_prefix) = existing.strip_prefix(&prefix) else {
                continue;
            };
            let Some((params, return_type)) = Self::method_signature_parts(after_prefix) else {
                continue;
            };
            let method_value_type = format!("({params}) => {return_type}");
            if method_value_type == property_value_type {
                return true;
            }
        }
        false
    }

    fn object_literal_method_name_prefixes(name_text: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut push_name = |name: String| {
            if !names.contains(&name) {
                names.push(name);
            }
        };
        push_name(name_text.to_string());
        if let Some(unquoted) = name_text
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .or_else(|| {
                name_text
                    .strip_prefix('\'')
                    .and_then(|name| name.strip_suffix('\''))
            })
        {
            push_name(unquoted.to_string());
            push_name(format!("\"{unquoted}\""));
            push_name(format!("'{unquoted}'"));
        } else if Self::is_unquoted_property_name(name_text) {
            push_name(format!("\"{name_text}\""));
            push_name(format!("'{name_text}'"));
        }
        names.into_iter().map(|name| format!("{name}(")).collect()
    }

    fn method_signature_parts(after_open_paren: &str) -> Option<(&str, &str)> {
        let bytes = after_open_paren.as_bytes();
        let mut angle_depth = 0usize;
        let mut paren_depth = 1usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut quote = None;

        for (idx, byte) in bytes.iter().copied().enumerate() {
            if let Some(q) = quote {
                if byte == b'\\' {
                    continue;
                }
                if byte == q {
                    quote = None;
                }
                continue;
            }
            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'(' => paren_depth += 1,
                b')' => {
                    paren_depth = paren_depth.saturating_sub(1);
                    if paren_depth == 0
                        && angle_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                    {
                        let params = after_open_paren[..idx].trim();
                        let return_type = after_open_paren[idx + 1..]
                            .trim_start()
                            .strip_prefix(':')?
                            .trim();
                        return Some((params, return_type));
                    }
                }
                _ => {}
            }
        }
        None
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
        if Self::object_literal_method_line_matches_name(trimmed, name_text) {
            return true;
        }

        for prefix in Self::object_literal_property_name_prefixes(name_text) {
            if trimmed.starts_with(&prefix) || trimmed.starts_with(&format!("readonly {prefix}")) {
                return true;
            }
        }

        false
    }

    fn is_symbol_observer_computed_property_name_text(name_text: &str) -> bool {
        name_text.trim_start().starts_with("[Symbol.observer]")
    }

    fn is_negative_numeric_computed_property_name_text(name_text: &str) -> bool {
        Self::negative_numeric_computed_property_key_text(name_text).is_some()
    }

    fn negative_numeric_computed_property_key_text(name_text: &str) -> Option<&str> {
        let inner = name_text
            .trim()
            .strip_prefix("[-")
            .and_then(|name| name.strip_suffix(']'))?;

        inner.parse::<f64>().ok()?;
        name_text.trim().strip_prefix('[')?.strip_suffix(']')
    }

    fn replace_object_literal_property_line_name(
        line: &mut String,
        key_text: &str,
        replacement_name: &str,
    ) -> bool {
        let leading_len = line.len() - line.trim_start().len();
        let trimmed = &line[leading_len..];
        let candidates = [
            format!("\"{key_text}\":"),
            format!("'{key_text}':"),
            format!("{key_text}:"),
        ];
        for candidate in candidates {
            if trimmed.starts_with(&candidate) {
                let replacement = format!("{replacement_name}:");
                line.replace_range(leading_len..leading_len + candidate.len(), &replacement);
                return true;
            }
        }
        false
    }

    pub(in crate::declaration_emitter) fn object_literal_line_matches_any_name(
        existing: &str,
        names: &[String],
    ) -> bool {
        names.iter().any(|name| {
            Self::object_literal_property_name_prefixes(name)
                .into_iter()
                .any(|prefix| {
                    existing.starts_with(&prefix)
                        || existing.starts_with(&format!("readonly {prefix}"))
                })
        })
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
        if let Some(method) = self.arena.get_method_decl(member_node)
            && (method.type_annotation.is_some()
                || method.parameters.nodes.iter().copied().any(|param_idx| {
                    self.arena
                        .get(param_idx)
                        .and_then(|param_node| self.arena.get_parameter(param_node))
                        .is_some_and(|param| {
                            param.type_annotation.is_some()
                                && self.arena.get(param.name).is_some_and(|name_node| {
                                    name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                })
                        })
                }))
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
        if self.explicit_asserted_type_text(initializer).is_some() {
            return true;
        }
        if self
            .preferred_expression_type_text(initializer)
            .is_some_and(|text| {
                !text.is_empty()
                    && text != "any"
                    && (Self::type_text_contains_import_type(&text) || text.starts_with("typeof "))
            })
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
        // Shorthand `{ foo }` has no initializer; `{ foo = expr }` is CoverInitializedName.
        self.arena.get_shorthand_property(member_node).map(|data| {
            if data.object_assignment_initializer == NodeIndex::NONE {
                data.name
            } else {
                data.object_assignment_initializer
            }
        })
    }

    pub(in crate::declaration_emitter) fn is_numeric_property_name_text(name: &str) -> bool {
        name.parse::<f64>().is_ok()
            || (name.starts_with("[-")
                && name.ends_with(']')
                && name[2..name.len().saturating_sub(1)].parse::<f64>().is_ok())
    }

    pub(in crate::declaration_emitter) fn is_numeric_like_object_property_line(line: &str) -> bool {
        let Some((name, _)) = line.split_once(':') else {
            return false;
        };
        let trimmed = name.trim().trim_start_matches("readonly ").trim();
        let normalized = trimmed
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                trimmed
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(trimmed);
        normalized.parse::<f64>().is_ok()
    }
}
