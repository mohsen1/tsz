//! Source-call type-parameter substitution helpers for declaration emit.

use super::super::DeclarationEmitter;
use super::escape_string_for_double_quote;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn source_function_body_contains_direct_call_to_name(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        name: &str,
    ) -> bool {
        if name.is_empty() {
            return false;
        }
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return false;
        };
        let Some(body_node) = source_arena.get(func.body) else {
            return false;
        };
        let Ok(start) = usize::try_from(body_node.pos) else {
            return false;
        };
        let Ok(end) = usize::try_from(body_node.end) else {
            return false;
        };
        let Some(body_text) = source_file.text.get(start..end) else {
            return false;
        };

        let mut pos = 0usize;
        while let Some(found) = body_text[pos..].find(name) {
            let abs_start = pos + found;
            let abs_end = abs_start + name.len();
            // Left word boundary: the character before `name` must not be an identifier char.
            // This prevents e.g. a function named "f" from matching the "f" inside "if (".
            let at_word_start = abs_start == 0 || {
                let prev = body_text.as_bytes()[abs_start - 1];
                !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'$'
            };
            if at_word_start {
                let after_name = &body_text[abs_end..];
                let after_ws = after_name.trim_start();
                if after_ws.starts_with('(') || after_ws.starts_with('<') {
                    return true;
                }
            }
            pos = abs_end;
        }
        false
    }

    pub(in crate::declaration_emitter) fn function_body_returned_parameter_call_return_type_text(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = source_arena.get(func.body)?;
        let block = source_arena.get_block(body_node)?;
        if block.statements.nodes.len() != 1 {
            return None;
        }
        let stmt_node = source_arena.get(*block.statements.nodes.first()?)?;
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return None;
        }
        let ret = source_arena.get_return_statement(stmt_node)?;
        let return_expr = self.skip_parenthesized_expression(ret.expression)?;
        let call_node = source_arena.get(return_expr)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = source_arena.get_call_expr(call_node)?;
        let callee_idx = self.skip_parenthesized_expression(call.expression)?;
        let callee_node = source_arena.get(callee_idx)?;
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let callee_name = self.identifier_text_from_arena(source_arena, callee_idx)?;
        for &param_idx in &func.parameters.nodes {
            let param_node = source_arena.get(param_idx)?;
            let param = source_arena.get_parameter(param_node)?;
            if self
                .identifier_text_from_arena(source_arena, param.name)
                .as_deref()
                != Some(callee_name.as_str())
            {
                continue;
            }
            let param_type_text = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))?;
            let parts = Self::parse_function_type_text(&param_type_text)?;
            return Some(parts.return_type);
        }
        None
    }

    pub(in crate::declaration_emitter) fn source_return_type_mentions_type_parameter(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        type_text: &str,
    ) -> bool {
        let Some(type_params) = func.type_parameters.as_ref() else {
            return false;
        };
        type_params.nodes.iter().copied().any(|param_idx| {
            source_arena
                .get(param_idx)
                .and_then(|param_node| source_arena.get_type_parameter(param_node))
                .and_then(|param| self.identifier_text_from_arena(source_arena, param.name))
                .is_some_and(|name| Self::contains_whole_word_in_text(type_text, &name))
        })
    }

    /// True when the function's return type annotation is a *simple* type-
    /// parameter surface: a bare reference to one of its own declared type
    /// parameters (the `T` in `unboxify<T>(x: Boxified<T>): T` or `foo1<T>(…): T`),
    /// or an array of such a reference (the `U[]` in `map<T,U>(…): U[]`).
    ///
    /// In those shapes the inferred return type IS just the resolved type
    /// parameter (optionally inside an array), so tsc emits exactly that type
    /// with no surrounding *source* structure to preserve. Composite returns
    /// such as `D & M` (intersection), `T | U` (union), or `Foo<T>`
    /// (application) are intentionally excluded: tsc keeps their source-level
    /// structure, which the canonical printer would flatten, reorder, or merge.
    pub(in crate::declaration_emitter) fn source_return_is_bare_type_parameter(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> bool {
        let Some(annotation_idx) = func.type_annotation.into_option() else {
            return false;
        };
        self.type_node_is_type_parameter_or_array_of(source_arena, func, annotation_idx)
    }

    pub(in crate::declaration_emitter) fn source_return_rest_parameter_type_parameter_name(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let annotation_idx = func.type_annotation.into_option()?;
        let return_name =
            self.type_node_bare_type_parameter_name(source_arena, func, annotation_idx)?;
        func.parameters
            .nodes
            .iter()
            .copied()
            .any(|param_idx| {
                source_arena
                    .get(param_idx)
                    .and_then(|param_node| source_arena.get_parameter(param_node))
                    .is_some_and(|param| {
                        param.dot_dot_dot_token
                            && self
                                .type_node_bare_type_parameter_name(
                                    source_arena,
                                    func,
                                    param.type_annotation,
                                )
                                .as_deref()
                                == Some(return_name.as_str())
                    })
            })
            .then_some(return_name)
    }

    fn type_node_is_type_parameter_or_array_of(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        type_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        let Some(type_node) = source_arena.get(type_idx) else {
            return false;
        };
        // `T[]` — recurse on the element type so `U[]` (and `U[][]`) qualify
        // while keeping the same "no composite structure" guarantee.
        if type_node.kind == syntax_kind_ext::ARRAY_TYPE {
            let Some(array) = source_arena.get_array_type(type_node) else {
                return false;
            };
            return self.type_node_is_type_parameter_or_array_of(
                source_arena,
                func,
                array.element_type,
            );
        }
        let Some(name) = self.type_node_bare_type_parameter_name(source_arena, func, type_idx)
        else {
            return false;
        };
        !name.is_empty()
    }

    fn type_node_bare_type_parameter_name(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        type_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<String> {
        let type_node = source_arena.get(type_idx)?;
        // A bare type-parameter reference is a plain identifier or a type
        // reference with no type arguments naming a declared type parameter.
        let name = if type_node.kind == SyntaxKind::Identifier as u16 {
            self.identifier_text_from_arena(source_arena, type_idx)
        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = source_arena.get_type_ref(type_node)?;
            if type_ref.type_arguments.is_some() {
                return None;
            }
            self.identifier_text_from_arena(source_arena, type_ref.type_name)
        } else {
            None
        }?;
        func.type_parameters
            .as_ref()
            .is_some_and(|type_params| {
                type_params.nodes.iter().copied().any(|param_idx| {
                    source_arena
                        .get(param_idx)
                        .and_then(|param_node| source_arena.get_type_parameter(param_node))
                        .and_then(|param| self.identifier_text_from_arena(source_arena, param.name))
                        .is_some_and(|param_name| param_name == name)
                })
            })
            .then_some(name)
    }

    pub(in crate::declaration_emitter) fn substitute_source_call_type_parameters(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
        mut type_text: String,
    ) -> Option<String> {
        if let Some(evaluated) = self.evaluate_source_template_infer_conditional_call(
            source_arena,
            func,
            call,
            &type_text,
        ) {
            return Some(evaluated);
        }

        let Some(type_params) = func.type_parameters.as_ref() else {
            return Some(type_text);
        };
        if type_params.nodes.is_empty() {
            return Some(type_text);
        }

        let mut type_param_names = Vec::new();
        let mut type_param_constraints = Vec::new();
        let mut type_param_defaults = Vec::new();
        for &param_idx in &type_params.nodes {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_text) = self.identifier_text_from_arena(source_arena, param.name) else {
                continue;
            };
            if param.constraint.is_some()
                && let Some(constraint) = self
                    .emit_type_node_text_from_arena(source_arena, param.constraint)
                    .or_else(|| self.source_slice_from_arena(source_arena, param.constraint))
            {
                type_param_constraints.push((name_text.clone(), constraint));
            }
            if param.default.is_some()
                && let Some(default_text) = self
                    .emit_type_node_text_from_arena(source_arena, param.default)
                    .or_else(|| self.source_slice_from_arena(source_arena, param.default))
            {
                type_param_defaults.push((name_text.clone(), default_text));
            }
            type_param_names.push(name_text);
        }

        let return_type_param_name = type_param_names
            .iter()
            .find(|name| type_text.trim() == name.as_str())
            .cloned();
        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());
        let mut substitutions = if explicit_type_args.is_empty() {
            self.infer_call_type_param_substitutions_from_arguments(
                source_arena,
                &func.parameters,
                call,
                &type_param_names,
                &type_param_constraints,
            )
        } else {
            type_param_names
                .iter()
                .zip(explicit_type_args.iter())
                .map(|(name_text, arg_text)| (name_text.clone(), arg_text.clone()))
                .collect()
        };
        if explicit_type_args.is_empty()
            && let Some(name_text) = return_type_param_name.as_deref()
        {
            match self.literal_direct_type_parameter_argument_substitution(
                source_arena,
                func,
                call,
                name_text,
            ) {
                Some(Some(literal_text)) => {
                    Self::replace_or_push_substitution(&mut substitutions, name_text, literal_text);
                }
                Some(None) => {
                    // Conflict detected: clear any literal the argument-inference pass
                    // pre-inferred for this type param so the return type stays
                    // unsubstituted and the caller falls back to the constraint.
                    substitutions.retain(|(name, _)| name.as_str() != name_text);
                }
                None => {}
            }
        }
        for (name_text, default_text) in type_param_defaults {
            if substitutions
                .iter()
                .any(|(substituted, _)| substituted == &name_text)
                || !Self::contains_whole_word_in_text(&type_text, &name_text)
            {
                continue;
            }
            let default_text = Self::replace_whole_words_in_text(&default_text, &substitutions);
            substitutions.push((name_text, default_text));
        }
        if substitutions.is_empty()
            && type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
        {
            return None;
        }
        type_text = Self::expand_tuple_index_substitutions_text(&type_text, &substitutions);
        type_text = Self::replace_whole_words_in_text(&type_text, &substitutions);
        type_text = Self::flatten_tuple_spread_substitutions_text(&type_text);
        type_text = Self::simplify_string_literal_template_type_text(&type_text);
        type_text = Self::expand_literal_key_mapped_type_text(&type_text).unwrap_or(type_text);
        if type_param_names
            .iter()
            .any(|name| Self::contains_whole_word_in_text(&type_text, name))
        {
            return None;
        }
        if type_text.contains("unknown") {
            return None;
        }
        Some(type_text)
    }

    /// Returns:
    /// - `Some(Some(literal))` — found an unambiguous direct-T literal; use it.
    /// - `Some(None)` — found a direct-T parameter with a literal argument, but a
    ///   conflicting object-property inference site prevents committing to it.
    ///   The caller should clear any pre-inferred substitution for `type_param_name`
    ///   so the return type falls back to the constraint.
    /// - `None` — no direct-T parameter with a primitive literal argument was found;
    ///   leave existing substitutions untouched.
    pub(in crate::declaration_emitter) fn literal_direct_type_parameter_argument_substitution(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
        type_param_name: &str,
    ) -> Option<Option<String>> {
        let args = call.arguments.as_ref()?;
        let params = &func.parameters.nodes;
        let args_nodes = &args.nodes;
        let mut found_candidate = false;
        for (candidate_pos, (&param_idx, &arg_idx)) in
            params.iter().zip(args_nodes.iter()).enumerate()
        {
            let param_node = source_arena.get(param_idx)?;
            let param = source_arena.get_parameter(param_node)?;
            if param.dot_dot_dot_token {
                continue;
            }
            let param_type_text = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))?;
            if param_type_text.trim() != type_param_name {
                continue;
            }
            // Compute the candidate literal early so the conflict test can compare
            // against it.  If this parameter has no primitive literal argument there
            // is nothing to preserve, so skip it.
            let Some(candidate_literal) = self.primitive_literal_argument_type_text(arg_idx) else {
                continue;
            };
            found_candidate = true;
            // A conflicting inference site requires THREE conditions:
            //
            // (1) Another parameter's type annotation has T in a direct
            //     object-property position (e.g. `options: { type?: T }`).
            // (2) The corresponding call argument is a non-empty object literal
            //     (`{}`, `undefined`, and omitted optional args do not contribute).
            // (3) That object literal has a property whose type is T and whose
            //     value is a *different* primitive literal than the candidate.
            //     Same-literal contributions (e.g. `f("x", { type: "x" })`) are
            //     not a conflict — tsc keeps the literal in that case.
            //
            // Callback positions (`(x: T) => R`, method signatures, and generic
            // aliases `Callback<T>`) are excluded by the structural annotation walk.
            let has_conflicting_site = params
                .iter()
                .copied()
                .enumerate()
                .filter(|&(i, _)| i != candidate_pos)
                .any(|(i, other_param_idx)| {
                    let Some(other_arg_idx) = args_nodes.get(i).copied() else {
                        return false;
                    };
                    source_arena
                        .get(other_param_idx)
                        .and_then(|node| source_arena.get_parameter(node))
                        .is_some_and(|other_param| {
                            object_arg_has_property_with_different_literal(
                                source_arena,
                                &self.arena,
                                other_param.type_annotation,
                                other_arg_idx,
                                type_param_name,
                                &candidate_literal,
                            )
                        })
                });
            if !has_conflicting_site {
                return Some(Some(candidate_literal));
            }
        }
        // Distinguish "conflict detected" (Some(None)) from "no direct-T param" (None)
        // so callers can actively clear a pre-inferred literal substitution on conflict.
        if found_candidate { Some(None) } else { None }
    }

    /// When the inferred return type is a bare type parameter whose substitution is a
    /// primitive literal AND a conflicting object-property inference site is present,
    /// clears the pre-inferred literal from `substitutions` so the caller's fallback
    /// loop can substitute the constraint instead (e.g. `"three"` → `string`).
    pub(in crate::declaration_emitter) fn clear_conflicting_literal_substitution(
        &self,
        source_arena: &NodeArena,
        decl_idx: NodeIndex,
        call: &tsz_parser::parser::node::CallExprData,
        type_text: &str,
        type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let Some(return_name) = type_param_names
            .iter()
            .find(|n| type_text.trim() == n.as_str())
        else {
            return;
        };
        if !substitutions
            .iter()
            .any(|(n, v)| n == return_name && Self::is_literal_type_text_for_const_call(v))
        {
            return;
        }
        let Some(func) = self.callable_function_from_symbol_decl(source_arena, decl_idx) else {
            return;
        };
        if matches!(
            self.literal_direct_type_parameter_argument_substitution(
                source_arena,
                func,
                call,
                return_name,
            ),
            Some(None)
        ) {
            substitutions.retain(|(n, _)| n != return_name);
        }
    }

    pub(in crate::declaration_emitter) fn simple_type_parameter_argument_substitution(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
        type_param_name: &str,
    ) -> Option<String> {
        let args = call.arguments.as_ref()?;
        for (&param_idx, &arg_idx) in func.parameters.nodes.iter().zip(args.nodes.iter()) {
            let param_node = source_arena.get(param_idx)?;
            let param = source_arena.get_parameter(param_node)?;
            if param.dot_dot_dot_token {
                continue;
            }
            let param_type_text = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))?;
            let Some((param_wrapper, param_inner)) =
                Self::single_generic_type_argument_text(param_type_text.trim())
            else {
                continue;
            };
            if param_inner != type_param_name {
                continue;
            }
            let arg_type_text = self
                .lexical_parameter_declared_type_annotation_text(arg_idx)
                .or_else(|| self.referenced_parameter_declared_type_annotation_text(arg_idx))
                .or_else(|| self.reference_declared_source_type_annotation_text(arg_idx))
                .or_else(|| self.reference_declared_type_annotation_text(arg_idx))?;
            let Some((arg_wrapper, arg_inner)) =
                Self::single_generic_type_argument_text(arg_type_text.trim())
            else {
                continue;
            };
            if param_wrapper == arg_wrapper && Self::type_text_is_simple_reference(arg_inner) {
                return Some(arg_inner.to_string());
            }
        }
        None
    }

    fn replace_or_push_substitution(
        substitutions: &mut Vec<(String, String)>,
        name: &str,
        value: String,
    ) {
        if let Some((_, existing)) = substitutions
            .iter_mut()
            .find(|(known, _)| known.as_str() == name)
        {
            *existing = value;
            return;
        }
        substitutions.push((name.to_string(), value));
    }

    pub(in crate::declaration_emitter) fn function_has_higher_order_type_parameter_parameter(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        type_param_name: &str,
    ) -> bool {
        if type_param_name.is_empty() {
            return false;
        }
        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            if !Self::contains_whole_word_in_text(&param_type_text, type_param_name) {
                continue;
            }
            let Some(parts) = Self::parse_function_type_text(&param_type_text) else {
                continue;
            };
            if parts.parameters.iter().any(|param| {
                Self::contains_whole_word_in_text(&param.type_text, type_param_name)
                    && Self::parse_function_type_text(&param.type_text).is_some()
            }) {
                return true;
            }
        }
        false
    }

    fn expand_literal_key_mapped_type_text(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?.trim();
        let mapped = inner.strip_prefix('[')?;
        let in_pos = mapped.find(" in ")?;
        let after_in = mapped.get(in_pos + " in ".len()..)?;
        let end_bracket = after_in.find(']')?;
        let keys_text = after_in.get(..end_bracket)?.trim();
        let after_bracket = after_in.get(end_bracket + 1..)?.trim();
        let value_text = after_bracket
            .strip_prefix(':')?
            .trim()
            .trim_end_matches(';')
            .trim();
        if value_text.is_empty() {
            return None;
        }
        let mut lines = Vec::new();
        for key in Self::split_top_level_union_type_parts(keys_text) {
            let key = key.trim();
            let key = Self::unquoted_string_literal_text(key)?;
            if !Self::is_simple_identifier_text(&key) {
                return None;
            }
            lines.push(format!("    {key}: {value_text};"));
        }
        (!lines.is_empty()).then(|| format!("{{\n{}\n}}", lines.join("\n")))
    }

    fn simplify_string_literal_template_type_text(type_text: &str) -> String {
        let mut output = String::with_capacity(type_text.len());
        let bytes = type_text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'`' {
                output.push(bytes[i] as char);
                i += 1;
                continue;
            }
            if let Some((replacement, next)) = Self::try_simplify_template_literal_at(type_text, i)
            {
                output.push_str(&replacement);
                i = next;
            } else if let Some(end) = type_text.get(i + 1..).and_then(|text| text.find('`')) {
                let end = i + 1 + end + 1;
                output.push_str(type_text.get(i..end).unwrap_or("`"));
                i = end;
            } else {
                output.push('`');
                i += 1;
            }
        }
        output
    }

    fn try_simplify_template_literal_at(type_text: &str, start: usize) -> Option<(String, usize)> {
        let bytes = type_text.as_bytes();
        let mut i = start + 1;
        let mut value = String::new();
        while i < bytes.len() {
            match bytes[i] {
                b'`' => return Some((format!("{value:?}"), i + 1)),
                b'$' if bytes.get(i + 1) == Some(&b'{') => {
                    let expr_start = i + 2;
                    let expr_end = type_text.get(expr_start..)?.find('}')? + expr_start;
                    let literal = type_text.get(expr_start..expr_end)?.trim();
                    let literal = Self::unquoted_string_literal_text(literal)?;
                    value.push_str(&literal);
                    i = expr_end + 1;
                }
                b'\\' => return None,
                byte => {
                    value.push(byte as char);
                    i += 1;
                }
            }
        }
        None
    }

    fn unquoted_string_literal_text(literal: &str) -> Option<String> {
        let quote = literal.as_bytes().first().copied()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        if literal.as_bytes().last().copied() != Some(quote) {
            return None;
        }
        Some(literal.get(1..literal.len() - 1)?.to_string())
    }

    pub(in crate::declaration_emitter) fn evaluate_source_template_infer_conditional_call(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
        type_text: &str,
    ) -> Option<String> {
        let (type_param_name, prefix, suffix, false_branch) =
            Self::parse_template_infer_conditional_text(type_text)?;
        if false_branch != "unknown" {
            return None;
        }

        let arguments = call.arguments.as_ref()?;
        let param_index = func.parameters.nodes.iter().position(|&param_idx| {
            let Some(param_node) = source_arena.get(param_idx) else {
                return false;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                return false;
            };
            self.emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
                .is_some_and(|text| text.trim() == type_param_name)
        })?;
        let arg_idx = *arguments.nodes.get(param_index)?;

        self.evaluate_template_infer_argument(arg_idx, &prefix, &suffix)
    }

    fn parse_template_infer_conditional_text(
        type_text: &str,
    ) -> Option<(String, String, String, String)> {
        let trimmed = type_text.trim();
        let (check_type, rest) = trimmed.split_once(" extends ")?;
        let (pattern_text, branches) = rest.split_once(" ? ")?;
        let (true_branch, false_branch) = branches.split_once(" : ")?;

        let pattern = pattern_text.trim().strip_prefix('`')?.strip_suffix('`')?;
        let infer_marker = "${infer ";
        let infer_start = pattern.find(infer_marker)?;
        let infer_name_start = infer_start + infer_marker.len();
        let infer_name_end = pattern.get(infer_name_start..)?.find('}')? + infer_name_start;
        let infer_name = pattern.get(infer_name_start..infer_name_end)?.trim();
        if infer_name.is_empty() || true_branch.trim() != infer_name {
            return None;
        }

        let prefix = pattern.get(..infer_start)?.to_string();
        let suffix = pattern.get(infer_name_end + 1..)?.to_string();
        Some((
            check_type.trim().to_string(),
            prefix,
            suffix,
            false_branch.trim().to_string(),
        ))
    }

    fn evaluate_template_infer_argument(
        &self,
        arg_idx: tsz_parser::parser::NodeIndex,
        prefix: &str,
        suffix: &str,
    ) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        match arg_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let literal = self.arena.get_literal(arg_node)?;
                Some(Self::template_infer_capture_text(
                    &literal.text,
                    prefix,
                    suffix,
                ))
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(literal) = self.const_string_literal_initializer_for_identifier(arg_idx)
                {
                    return Some(Self::template_infer_capture_text(&literal, prefix, suffix));
                }
                Some("unknown".to_string())
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.template_expression_infer_capture_text(arg_idx, prefix, suffix)
            }
            _ => None,
        }
    }

    fn const_string_literal_initializer_for_identifier(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.all_declarations() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !self.arena.is_const_variable_declaration(decl_idx) {
                continue;
            }
            let Some(init_node) = self.arena.get(decl.initializer) else {
                continue;
            };
            if init_node.kind == SyntaxKind::StringLiteral as u16
                || init_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                return self
                    .arena
                    .get_literal(init_node)
                    .map(|lit| lit.text.clone());
            }
        }
        None
    }

    fn template_expression_infer_capture_text(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
        prefix: &str,
        suffix: &str,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let template = self.arena.get_template_expr(expr_node)?;
        let spans = &template.template_spans.nodes;
        if spans.len() != 1 || !suffix.is_empty() {
            return None;
        }
        let head_node = self.arena.get(template.head)?;
        let head_text = self.arena.get_literal(head_node)?.text.as_str();
        if head_text != prefix {
            return Some("unknown".to_string());
        }
        let span_node = self.arena.get(spans[0])?;
        let span = self.arena.get_template_span(span_node)?;
        let tail_node = self.arena.get(span.literal)?;
        if self.arena.get_literal(tail_node)?.text.as_str() != suffix {
            return Some("unknown".to_string());
        }

        self.template_expression_hole_type_text(span.expression)
            .map(|text| Self::normalize_string_literal_union_quotes(&text))
    }

    fn template_expression_hole_type_text(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<String> {
        self.reference_declared_type_annotation_text(expr_idx)
            .or_else(|| self.const_literal_initializer_text(expr_idx))
            .or_else(|| {
                self.get_node_type_or_names(&[expr_idx])
                    .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            })
            .filter(|text| text != "any" && text != "unknown")
    }

    fn template_infer_capture_text(value: &str, prefix: &str, suffix: &str) -> String {
        let Some(captured) = value
            .strip_prefix(prefix)
            .and_then(|text| text.strip_suffix(suffix))
        else {
            return "unknown".to_string();
        };
        let escaped = super::escape_string_for_double_quote(captured);
        format!("\"{escaped}\"")
    }

    fn normalize_string_literal_union_quotes(type_text: &str) -> String {
        let parts = Self::split_top_level_union_type_parts(type_text);
        if parts.len() <= 1 {
            return Self::normalize_string_literal_quotes(type_text.trim());
        }
        parts
            .iter()
            .map(|part| Self::normalize_string_literal_quotes(part))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn normalize_string_literal_quotes(type_text: &str) -> String {
        let trimmed = type_text.trim();
        if trimmed.len() >= 2
            && trimmed.starts_with('\'')
            && trimmed.ends_with('\'')
            && !trimmed[1..trimmed.len() - 1].contains('\'')
        {
            let inner = &trimmed[1..trimmed.len() - 1];
            let escaped = super::escape_string_for_double_quote(inner);
            format!("\"{escaped}\"")
        } else {
            trimmed.to_string()
        }
    }

    pub(in crate::declaration_emitter) fn substitute_call_result_parameter_type_queries(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> String {
        if !source_type_text.contains("typeof ") {
            return source_type_text.to_string();
        }

        let mut text = source_type_text.to_string();
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
            if !Self::type_text_can_substitute_type_query_parameter(&param_type_text) {
                continue;
            }
            text = Self::replace_typeof_identifier(&text, &param_name, &param_type_text).0;
        }
        text
    }

    fn type_text_can_substitute_type_query_parameter(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        if Self::simple_type_reference_name(trimmed).is_some() {
            return true;
        }
        if matches!(trimmed, "true" | "false" | "null" | "undefined") {
            return true;
        }
        if trimmed.parse::<f64>().is_ok() {
            return true;
        }
        if trimmed.len() >= 2 {
            let bytes = trimmed.as_bytes();
            return (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'');
        }
        false
    }
}

/// Returns `true` when `type_idx` (the annotation of a parameter other than the
/// candidate direct-literal parameter) carries the given type parameter in a
/// **direct object-property inference position** — specifically as the type of a
/// `PropertySignature` inside a type literal.
///
/// Excluded (returns `false`):
/// - `MethodSignature` members — indirect callback-style inference.
/// - `FunctionType` / `ConstructorType` nodes — explicit callback annotations.
/// - Plain `TypeReference` wrappers like `Callback<T>` — generic alias, not a
///   type-literal property.
///
/// Recurses through `UnionType`, `IntersectionType`, `ParenthesizedType`, and
/// `OptionalType` wrappers so that `{ type?: T } | undefined` is still detected.
fn type_node_has_object_property_site_for(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    type_param_name: &str,
    depth: u8,
) -> bool {
    if depth > 16 {
        return false;
    }
    let Some(type_node) = source_arena.get(type_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == syntax_kind_ext::TYPE_LITERAL => {
            source_arena.get_type_literal(type_node).is_some_and(|lit| {
                lit.members.nodes.iter().copied().any(|member_idx| {
                    let Some(member_node) = source_arena.get(member_idx) else {
                        return false;
                    };
                    // Only PropertySignature is a direct inference position.
                    // MethodSignature (and CallSignature) are callback-like.
                    if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                        return false;
                    }
                    source_arena.get_signature(member_node).is_some_and(|sig| {
                        type_annotation_is_or_contains_type_param(
                            source_arena,
                            sig.type_annotation,
                            type_param_name,
                            depth + 1,
                        )
                    })
                })
            })
        }
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
            source_arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    composite.types.nodes.iter().copied().any(|part_idx| {
                        type_node_has_object_property_site_for(
                            source_arena,
                            part_idx,
                            type_param_name,
                            depth + 1,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            source_arena
                .get_wrapped_type(type_node)
                .is_some_and(|wrapped| {
                    type_node_has_object_property_site_for(
                        source_arena,
                        wrapped.type_node,
                        type_param_name,
                        depth + 1,
                    )
                })
        }
        // FunctionType, ConstructorType, TypeReference (e.g. Callback<T>), and all
        // other forms are not direct object-property inference positions.
        _ => false,
    }
}

/// Returns `true` when `type_idx` is, or recursively contains, a bare reference
/// to `type_param_name`.  Used to check a `PropertySignature`'s type annotation.
///
/// Recognises:
/// - A plain `Identifier` node equal to the name (uncommon but possible in some
///   serialised trees).
/// - A `TypeReference` with no type arguments whose name equals `type_param_name`
///   (the normal case for `T` in `{ prop: T }`).
/// - `UnionType` / `IntersectionType` / `ParenthesizedType` / `OptionalType`
///   wrappers (e.g. `T | undefined`, `(T)`).
///
/// Does NOT recurse into `FunctionType` parameters or return types.
fn type_annotation_is_or_contains_type_param(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    type_param_name: &str,
    depth: u8,
) -> bool {
    if depth > 16 {
        return false;
    }
    let Some(type_node) = source_arena.get(type_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == SyntaxKind::Identifier as u16 => source_arena
            .get_identifier(type_node)
            .is_some_and(|ident| ident.escaped_text == type_param_name),
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            let Some(type_ref) = source_arena.get_type_ref(type_node) else {
                return false;
            };
            // `Foo<T>` has type arguments — not a bare type-param reference.
            type_ref.type_arguments.is_none()
                && source_arena
                    .get(type_ref.type_name)
                    .and_then(|n| source_arena.get_identifier(n))
                    .is_some_and(|ident| ident.escaped_text == type_param_name)
        }
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
            source_arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    composite.types.nodes.iter().copied().any(|part_idx| {
                        type_annotation_is_or_contains_type_param(
                            source_arena,
                            part_idx,
                            type_param_name,
                            depth + 1,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE || k == syntax_kind_ext::OPTIONAL_TYPE => {
            source_arena
                .get_wrapped_type(type_node)
                .is_some_and(|wrapped| {
                    type_annotation_is_or_contains_type_param(
                        source_arena,
                        wrapped.type_node,
                        type_param_name,
                        depth + 1,
                    )
                })
        }
        // FunctionType and everything else — not a direct type-param reference.
        _ => false,
    }
}

/// Returns `true` when the argument at `arg_idx` is an object literal that
/// contains at least one `PropertyAssignment` whose name matches a T-typed
/// property in `annotation_idx` AND whose value is a primitive literal that
/// differs from `candidate_literal`.
///
/// This is the full call-site-aware conflict predicate:
/// - `{}`, `undefined`, and absent optional args → `false` (no inference).
/// - Same-literal property values (e.g. `{ type: "x" }` when candidate is
///   `"x"`) → `false` (no *conflicting* inference).
/// - Non-matching property names (e.g. `{ other: "y" }` against `{ type?: T }`)
///   → `false` (the T-typed property is absent from the argument).
/// - Different-literal property value (e.g. `{ type: "two" }` vs `"three"`)
///   → `true` (genuine conflict; tsc widens to constraint).
///
/// `annotation_arena` owns the parameter type-annotation nodes (`annotation_idx`).
/// `arg_arena` owns the call-argument nodes (`arg_idx`); this is always the
/// emitter's own arena, even when the callee's declaration lives in a different
/// arena (e.g. a global or re-exported symbol arena).
fn object_arg_has_property_with_different_literal(
    annotation_arena: &NodeArena,
    arg_arena: &NodeArena,
    annotation_idx: NodeIndex,
    arg_idx: NodeIndex,
    type_param_name: &str,
    candidate_literal: &str,
) -> bool {
    if !type_node_has_object_property_site_for(annotation_arena, annotation_idx, type_param_name, 0)
    {
        return false;
    }
    let Some(arg_node) = arg_arena.get(arg_idx) else {
        return false;
    };
    if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
        // `undefined` supplies no property value; any other non-object reference
        // (const variable, as-const binding, …) is opaque — treat conservatively
        // as a potential conflict.
        let is_undefined = arg_node.kind == SyntaxKind::Identifier as u16
            && arg_arena
                .get_identifier(arg_node)
                .is_some_and(|ident| ident.escaped_text == "undefined");
        return !is_undefined;
    }
    let Some(obj) = arg_arena.get_literal_expr(arg_node) else {
        return false;
    };
    if obj.elements.nodes.is_empty() {
        return false;
    }
    let mut t_prop_names: Vec<String> = Vec::new();
    collect_property_names_for_type_param(
        annotation_arena,
        annotation_idx,
        type_param_name,
        &mut t_prop_names,
        0,
    );
    if t_prop_names.is_empty() {
        return false;
    }
    obj.elements.nodes.iter().copied().any(|elem_idx| {
        let Some(elem_node) = arg_arena.get(elem_idx) else {
            return false;
        };
        if elem_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
            return false;
        }
        let Some(prop) = arg_arena.get_property_assignment(elem_node) else {
            return false;
        };
        let Some(prop_name_text) = arena_property_name_text(arg_arena, prop.name) else {
            return false;
        };
        if !t_prop_names
            .iter()
            .any(|n| n.as_str() == prop_name_text.as_str())
        {
            return false;
        }
        // This property carries T.  Check if its value differs from the candidate.
        let Some(val_node) = arg_arena.get(prop.initializer) else {
            return false;
        };
        let val_literal = match val_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => arg_arena
                .get_literal(val_node)
                .map(|lit| format!("\"{}\"", escape_string_for_double_quote(&lit.text))),
            k if k == SyntaxKind::NumericLiteral as u16 => {
                arg_arena.get_literal(val_node).map(|lit| lit.text.clone())
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            // Non-primitive value (identifier, expression, …) — cannot determine
            // the literal statically; treat conservatively as conflicting.
            _ => return true,
        };
        val_literal.is_some_and(|lit| lit != candidate_literal)
    })
}

/// Walks the type annotation at `type_idx` and appends to `names` the name of
/// every `PropertySignature` member whose type annotation is (or contains) a
/// direct reference to `type_param_name`.
fn collect_property_names_for_type_param(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    type_param_name: &str,
    names: &mut Vec<String>,
    depth: u8,
) {
    if depth > 16 {
        return;
    }
    let Some(type_node) = source_arena.get(type_idx) else {
        return;
    };
    match type_node.kind {
        k if k == syntax_kind_ext::TYPE_LITERAL => {
            let Some(lit) = source_arena.get_type_literal(type_node) else {
                return;
            };
            for member_idx in lit.members.nodes.iter().copied() {
                let Some(member_node) = source_arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    continue;
                }
                let Some(sig) = source_arena.get_signature(member_node) else {
                    continue;
                };
                if type_annotation_is_or_contains_type_param(
                    source_arena,
                    sig.type_annotation,
                    type_param_name,
                    depth + 1,
                ) {
                    if let Some(name) = arena_property_name_text(source_arena, sig.name) {
                        names.push(name);
                    }
                }
            }
        }
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
            let Some(composite) = source_arena.get_composite_type(type_node) else {
                return;
            };
            for part_idx in composite.types.nodes.iter().copied() {
                collect_property_names_for_type_param(
                    source_arena,
                    part_idx,
                    type_param_name,
                    names,
                    depth + 1,
                );
            }
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            let Some(wrapped) = source_arena.get_wrapped_type(type_node) else {
                return;
            };
            collect_property_names_for_type_param(
                source_arena,
                wrapped.type_node,
                type_param_name,
                names,
                depth + 1,
            );
        }
        _ => {}
    }
}

/// Returns the text of a property name node, handling both identifier and
/// quoted string/numeric literal forms.
///
/// In TypeScript `{ type: T }` and `{ "type": T }` declare the same property;
/// this helper unifies them so quoted-key annotation properties and quoted-key
/// object-literal arguments are matched correctly.
fn arena_property_name_text(source_arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = source_arena.get(idx)?;
    match node.kind {
        k if k == SyntaxKind::Identifier as u16 => source_arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.to_string()),
        k if k == SyntaxKind::StringLiteral as u16 || k == SyntaxKind::NumericLiteral as u16 => {
            source_arena.get_literal(node).map(|lit| lit.text.clone())
        }
        _ => None,
    }
}
