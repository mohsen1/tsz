//! String literal completion logic.
//!
//! Provides completions inside string literal positions: union string literal
//! types, `keyof` constraint labels, and type-parameter constraints that
//! resolve to string literal unions.

use rustc_hash::{FxHashMap, FxHashSet};

use super::*;
use tsz_binder::SymbolId;
use tsz_parser::parser::node::CallExprData;

impl<'a> Completions<'a> {
    pub(super) fn get_contextual_string_literal_completions(
        &self,
        node_idx: NodeIndex,
        offset: u32,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;
        let file_name = self.file_name.as_ref()?;
        let mut cache_ref = type_cache;
        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = cache_ref.as_deref_mut() {
            if let Some(cache_value) = cache.take() {
                CheckerState::with_cache(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    cache_value,
                    compiler_options,
                )
            } else {
                CheckerState::new(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    compiler_options,
                )
            }
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                interner,
                file_name.clone(),
                compiler_options,
            )
        };
        if !self.lib_contexts.is_empty() {
            checker.ctx.set_lib_contexts(self.lib_contexts.to_vec());
        }
        let mut expected = self.get_contextual_type(node_idx, &mut checker);
        if expected.is_none() {
            let mut current = node_idx;
            for _ in 0..8 {
                let Some(ext) = self.arena.get_extended(current) else {
                    break;
                };
                if !ext.parent.is_some() || ext.parent == current {
                    break;
                }
                current = ext.parent;
                expected = self.get_contextual_type(current, &mut checker);
                if expected.is_some() {
                    break;
                }
            }
        }
        let mut visited = FxHashSet::default();
        let mut labels = FxHashSet::default();
        if let Some(expected) = expected {
            self.collect_string_literal_candidates(
                expected,
                interner,
                &mut checker,
                &mut visited,
                &mut labels,
            );
        }
        self.collect_string_constraint_labels_from_call(node_idx, offset, &mut labels);
        if let Some(cache) = cache_ref {
            *cache = Some(checker.extract_cache());
        }
        if labels.is_empty() {
            return None;
        }
        let mut items: Vec<_> = labels
            .into_iter()
            .map(|label| {
                let mut item =
                    CompletionItem::new(format!("\"{label}\""), CompletionItemKind::Variable);
                item.sort_text = Some(sort_priority::LOCATION_PRIORITY.to_string());
                item
            })
            .collect();
        items.sort_by(|a, b| a.label.cmp(&b.label));
        Some(items)
    }

    fn collect_string_constraint_labels_from_call(
        &self,
        node_idx: NodeIndex,
        offset: u32,
        labels: &mut FxHashSet<String>,
    ) {
        let Some(call_idx) = self.find_enclosing_call_expression(node_idx).or_else(|| {
            self.find_enclosing_call_expression(crate::utils::find_node_at_offset(
                self.arena, offset,
            ))
        }) else {
            self.collect_string_constraint_labels_from_text_callsite(offset, labels);
            return;
        };
        let Some(call_node) = self.arena.get(call_idx) else {
            return;
        };
        let Some(call) = self.arena.get_call_expr(call_node) else {
            return;
        };
        let arg_index = self.resolve_call_argument_index(call, offset);

        let callee_symbol = self
            .resolve_member_target_symbol(call.expression)
            .or_else(|| {
                self.arena
                    .get_identifier_text(call.expression)
                    .and_then(|name| self.binder.file_locals.get(name))
            });
        let Some(callee_symbol) = callee_symbol else {
            return;
        };
        self.collect_string_constraint_labels_for_callee(callee_symbol, arg_index, labels);
    }

    fn find_enclosing_call_expression(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..25 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            if ext.parent == current || !ext.parent.is_some() {
                break;
            }
            current = ext.parent;
        }
        None
    }

    fn resolve_call_argument_index(&self, call: &CallExprData, offset: u32) -> usize {
        let Some(args) = call.arguments.as_ref() else {
            return 0;
        };
        if args.nodes.is_empty() {
            return 0;
        }

        let mut seen_non_omitted = 0usize;
        let mut last_non_omitted_end = None;
        for (i, &arg_idx) in args.nodes.iter().enumerate() {
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let arg_start = arg_node.pos;
            let arg_end = arg_node.end;
            if offset <= arg_start {
                return seen_non_omitted;
            }
            if offset <= arg_end {
                return seen_non_omitted;
            }
            let next_start = args
                .nodes
                .iter()
                .skip(i + 1)
                .filter_map(|&next_idx| self.arena.get(next_idx))
                .find(|next| next.kind != syntax_kind_ext::OMITTED_EXPRESSION)
                .map(|next| next.pos);
            if let Some(next_start) = next_start
                && offset < next_start
            {
                return seen_non_omitted + 1;
            }
            last_non_omitted_end = Some(arg_end as usize);
            seen_non_omitted += 1;
        }
        if let Some(last_end) = last_non_omitted_end {
            let end = (offset as usize).min(self.source_text.len());
            if last_end < end && seen_non_omitted > 0 {
                let gap = &self.source_text[last_end..end];
                if !gap.contains(',') {
                    return seen_non_omitted - 1;
                }
            }
        }
        seen_non_omitted
    }

    fn collect_string_constraint_labels_from_text_callsite(
        &self,
        offset: u32,
        labels: &mut FxHashSet<String>,
    ) {
        let source = self.source_text;
        let end = (offset as usize).min(source.len());
        let prefix = &source[..end];
        let Some(open_paren_idx) = prefix.rfind('(') else {
            return;
        };
        let arg_index = self.count_top_level_commas(open_paren_idx + 1, end);

        let mut i = open_paren_idx;
        while i > 0 && source.as_bytes()[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        let ident_end = i;
        while i > 0 {
            let ch = source.as_bytes()[i - 1];
            if ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'$' {
                i -= 1;
            } else {
                break;
            }
        }
        if i == ident_end {
            return;
        }
        let callee_name = &source[i..ident_end];
        let Some(callee_symbol) = self.binder.file_locals.get(callee_name) else {
            return;
        };
        self.collect_string_constraint_labels_for_callee(callee_symbol, arg_index, labels);
    }

    fn count_top_level_commas(&self, start: usize, end: usize) -> usize {
        if start >= end || end > self.source_text.len() {
            return 0;
        }
        let bytes = self.source_text.as_bytes();
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut commas = 0usize;
        let mut i = start;
        while i < end {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    commas += 1;
                }
                b'"' | b'\'' => {
                    let quote = bytes[i];
                    i += 1;
                    while i < end {
                        if bytes[i] == b'\\' {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == quote {
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        commas
    }

    fn collect_string_constraint_labels_for_callee(
        &self,
        callee_symbol: SymbolId,
        arg_index: usize,
        labels: &mut FxHashSet<String>,
    ) {
        let Some(callee) = self.binder.symbols.get(callee_symbol) else {
            return;
        };
        for &decl_idx in &callee.declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }
            let Some(func) = self.arena.get_function(decl_node) else {
                continue;
            };
            if arg_index >= func.parameters.nodes.len() {
                continue;
            }
            let param_idx = func.parameters.nodes[arg_index];
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !param.type_annotation.is_some() {
                continue;
            }
            let Some(param_type_node) = self.arena.get(param.type_annotation) else {
                continue;
            };
            let before_len = labels.len();
            if param_type_node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.arena.get_type_ref(param_type_node)
                && let Some(type_name) = self.arena.get_identifier_text(type_ref.type_name)
                && let Some(type_params) = func.type_parameters.as_ref()
            {
                for &type_param_idx in &type_params.nodes {
                    let Some(type_param_node) = self.arena.get(type_param_idx) else {
                        continue;
                    };
                    let Some(type_param) = self.arena.get_type_parameter(type_param_node) else {
                        continue;
                    };
                    let Some(type_param_name) = self.arena.get_identifier_text(type_param.name)
                    else {
                        continue;
                    };
                    if type_param_name != type_name || !type_param.constraint.is_some() {
                        continue;
                    }
                    self.collect_string_literals_from_type_node(type_param.constraint, labels);
                }
            } else {
                self.collect_string_literals_from_type_node(param.type_annotation, labels);
            }
            if labels.len() == before_len {
                self.collect_type_parameter_constraint_labels_from_type_node(
                    func.type_parameters.as_ref(),
                    param.type_annotation,
                    labels,
                );
            }
            if labels.len() == before_len
                && let Some(type_parameters) = func.type_parameters.as_ref()
            {
                for &type_param_idx in &type_parameters.nodes {
                    let Some(type_param_node) = self.arena.get(type_param_idx) else {
                        continue;
                    };
                    let Some(type_param) = self.arena.get_type_parameter(type_param_node) else {
                        continue;
                    };
                    if type_param.constraint.is_some() {
                        self.collect_string_literals_from_type_node(type_param.constraint, labels);
                    }
                }
            }
            if labels.len() == before_len {
                self.collect_string_literals_from_type_node(decl_idx, labels);
            }
        }
    }

    fn collect_type_parameter_constraint_labels_from_type_node(
        &self,
        type_parameters: Option<&tsz_parser::parser::base::NodeList>,
        type_node_idx: NodeIndex,
        labels: &mut FxHashSet<String>,
    ) {
        let Some(type_parameters) = type_parameters else {
            return;
        };
        let Some(type_node) = self.arena.get(type_node_idx) else {
            return;
        };
        let start = type_node.pos as usize;
        let end = (type_node.end as usize).min(self.source_text.len());
        if start >= end {
            return;
        }
        let type_text = &self.source_text[start..end];
        for &type_param_idx in &type_parameters.nodes {
            let Some(type_param_node) = self.arena.get(type_param_idx) else {
                continue;
            };
            let Some(type_param) = self.arena.get_type_parameter(type_param_node) else {
                continue;
            };
            if !type_param.constraint.is_some() {
                continue;
            }
            let Some(type_param_name) = self.arena.get_identifier_text(type_param.name) else {
                continue;
            };
            if !Self::contains_identifier(type_text, type_param_name) {
                continue;
            }
            self.collect_string_literals_from_type_node(type_param.constraint, labels);
        }
    }

    fn contains_identifier(text: &str, ident: &str) -> bool {
        if ident.is_empty() || text.len() < ident.len() {
            return false;
        }
        let mut search_start = 0usize;
        while let Some(rel_idx) = text[search_start..].find(ident) {
            let idx = search_start + rel_idx;
            let end = idx + ident.len();
            let before = (idx > 0).then(|| text.as_bytes()[idx - 1] as char);
            let after = (end < text.len()).then(|| text.as_bytes()[end] as char);
            let before_ok = before.is_none_or(|ch| !Self::is_identifier_char(ch));
            let after_ok = after.is_none_or(|ch| !Self::is_identifier_char(ch));
            if before_ok && after_ok {
                return true;
            }
            search_start = idx + ident.len();
        }
        false
    }

    const fn is_identifier_char(ch: char) -> bool {
        ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    }

    fn collect_string_literals_from_type_node(
        &self,
        type_node_idx: NodeIndex,
        labels: &mut FxHashSet<String>,
    ) {
        let Some(type_node) = self.arena.get(type_node_idx) else {
            return;
        };
        let start = type_node.pos as usize;
        let end = (type_node.end as usize).min(self.source_text.len());
        if start >= end {
            return;
        }
        let text = &self.source_text[start..end];
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let quote = bytes[i];
            if quote != b'"' && quote != b'\'' {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == quote {
                    let lit = &text[i + 1..j];
                    if !lit.is_empty() {
                        labels.insert(lit.to_string());
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            i = j;
        }
    }

    pub(super) fn get_string_literal_completions(
        &self,
        node_idx: NodeIndex,
        offset: u32,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;
        let file_name = self.file_name.as_ref()?;
        let string_literal_idx = self
            .find_enclosing_string_literal(node_idx)
            .or_else(|| self.string_literal_at_offset(offset))?;
        if self.is_module_specifier_string_literal(string_literal_idx) {
            return None;
        }

        let mut cache_ref = type_cache;
        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = cache_ref.as_deref_mut() {
            if let Some(cache_value) = cache.take() {
                CheckerState::with_cache(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    cache_value,
                    compiler_options,
                )
            } else {
                CheckerState::new(
                    self.arena,
                    self.binder,
                    interner,
                    file_name.clone(),
                    compiler_options,
                )
            }
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                interner,
                file_name.clone(),
                compiler_options,
            )
        };
        if !self.lib_contexts.is_empty() {
            checker.ctx.set_lib_contexts(self.lib_contexts.to_vec());
        }

        let expected = self.get_contextual_type(string_literal_idx, &mut checker)?;
        let mut visited = FxHashSet::default();
        let mut labels = FxHashSet::default();
        self.collect_string_literal_candidates(
            expected,
            interner,
            &mut checker,
            &mut visited,
            &mut labels,
        );
        if labels.is_empty() {
            self.collect_keyof_constraint_labels_from_call(string_literal_idx, &mut labels);
        }

        if let Some(cache) = cache_ref {
            *cache = Some(checker.extract_cache());
        }

        if labels.is_empty() {
            return None;
        }

        // Compute the replacement span: the content between quotes of the
        // enclosing string literal (start+1 .. end-1), matching TypeScript's
        // `createTextSpanFromStringLiteralLikeContent`.
        let replacement_span = self.arena.get(string_literal_idx).map(|node| {
            let start = node.pos + 1; // skip opening quote
            let end = if node.end > node.pos + 1 {
                node.end - 1 // skip closing quote
            } else {
                node.end
            };
            (start, end)
        });

        let mut items: Vec<_> = labels
            .into_iter()
            .map(|label| {
                let mut item = CompletionItem::new(label, CompletionItemKind::Variable);
                item.sort_text = Some(sort_priority::LOCATION_PRIORITY.to_string());
                if let Some((start, end)) = replacement_span {
                    item.replacement_span = Some((start, end));
                }
                item
            })
            .collect();
        items.sort_by(|a, b| a.label.cmp(&b.label));
        Some(items)
    }

    fn collect_string_literal_candidates(
        &self,
        type_id: TypeId,
        interner: &TypeInterner,
        checker: &mut CheckerState,
        visited: &mut FxHashSet<TypeId>,
        labels: &mut FxHashSet<String>,
    ) {
        if !visited.insert(type_id) {
            return;
        }

        let evaluated = tsz_solver::evaluate_type(interner, type_id);
        if evaluated != type_id {
            self.collect_string_literal_candidates(evaluated, interner, checker, visited, labels);
        }

        if let Some(tsz_solver::LiteralValue::String(text)) =
            visitor::literal_value(interner, type_id)
        {
            labels.insert(interner.resolve_atom(text));
            return;
        }

        if let Some(param_info) = visitor::type_param_info(interner, type_id)
            && let Some(constraint) = param_info.constraint
        {
            self.collect_string_literal_candidates(constraint, interner, checker, visited, labels);
            return;
        }

        if let Some(keyof_operand) = visitor::keyof_inner_type(interner, type_id) {
            let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
            let mut prop_visited = FxHashSet::default();
            self.collect_properties_for_type(
                keyof_operand,
                interner,
                checker,
                &mut prop_visited,
                &mut props,
            );
            labels.extend(props.into_keys());
            return;
        }

        if let Some(members) = visitor::union_list_id(interner, type_id)
            .or_else(|| visitor::intersection_list_id(interner, type_id))
        {
            let members = interner.type_list(members);
            for &member in members.iter() {
                self.collect_string_literal_candidates(member, interner, checker, visited, labels);
            }
            return;
        }

        if let Some(element_type) = visitor::array_element_type(interner, type_id) {
            self.collect_string_literal_candidates(
                element_type,
                interner,
                checker,
                visited,
                labels,
            );
            return;
        }

        if let Some(tuple_list_id) = visitor::tuple_list_id(interner, type_id) {
            let elements = interner.tuple_list(tuple_list_id);
            for element in elements.iter() {
                self.collect_string_literal_candidates(
                    element.type_id,
                    interner,
                    checker,
                    visited,
                    labels,
                );
            }
        }
    }

    fn find_enclosing_string_literal(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::StringLiteral as u16 {
            return Some(node_idx);
        }
        let ext = self.arena.get_extended(node_idx)?;
        let parent = self.arena.get(ext.parent)?;
        (parent.kind == SyntaxKind::StringLiteral as u16).then_some(ext.parent)
    }

    fn string_literal_at_offset(&self, offset: u32) -> Option<NodeIndex> {
        let direct = find_node_at_offset(self.arena, offset);
        if direct.is_some() {
            return self.find_enclosing_string_literal(direct);
        }
        if offset > 0 {
            let prev = find_node_at_offset(self.arena, offset - 1);
            if prev.is_some() {
                return self.find_enclosing_string_literal(prev);
            }
        }
        let next = find_node_at_offset(self.arena, offset.saturating_add(1));
        if next.is_some() {
            return self.find_enclosing_string_literal(next);
        }
        None
    }

    fn is_module_specifier_string_literal(&self, string_literal_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(string_literal_idx) else {
            return false;
        };
        let Some(parent) = self.arena.get(ext.parent) else {
            return false;
        };
        parent.kind == syntax_kind_ext::IMPORT_DECLARATION
            || parent.kind == syntax_kind_ext::EXPORT_DECLARATION
            || parent.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
    }

    fn collect_keyof_constraint_labels_from_call(
        &self,
        string_literal_idx: NodeIndex,
        labels: &mut FxHashSet<String>,
    ) {
        let mut call_idx = NodeIndex::NONE;
        let mut current = string_literal_idx;
        let mut depth = 0;
        while current.is_some() && depth < 50 {
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            let Some(parent) = self.arena.get(ext.parent) else {
                break;
            };
            if parent.kind == syntax_kind_ext::CALL_EXPRESSION {
                call_idx = ext.parent;
                break;
            }
            if ext.parent == current {
                break;
            }
            current = ext.parent;
            depth += 1;
        }
        if !call_idx.is_some() {
            return;
        }

        let Some(call_node) = self.arena.get(call_idx) else {
            return;
        };
        let Some(call) = self.arena.get_call_expr(call_node) else {
            return;
        };
        let arg_index = call.arguments.as_ref().and_then(|args| {
            args.nodes
                .iter()
                .position(|&arg| self.node_contains(arg, string_literal_idx))
        });
        let Some(arg_index) = arg_index else {
            return;
        };

        let callee_symbol = self
            .resolve_member_target_symbol(call.expression)
            .or_else(|| {
                self.arena
                    .get_identifier_text(call.expression)
                    .and_then(|name| self.binder.file_locals.get(name))
            });
        let Some(callee_symbol) = callee_symbol else {
            return;
        };
        let Some(callee) = self.binder.symbols.get(callee_symbol) else {
            return;
        };
        for &decl_idx in &callee.declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }
            let Some(func) = self.arena.get_function(decl_node) else {
                continue;
            };
            if arg_index >= func.parameters.nodes.len() {
                continue;
            }
            let param_node_idx = func.parameters.nodes[arg_index];
            let Some(param_node) = self.arena.get(param_node_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !param.type_annotation.is_some() {
                continue;
            }
            let param_type_name =
                if let Some(name) = self.arena.get_identifier_text(param.type_annotation) {
                    Some(name.to_string())
                } else if let Some(type_node) = self.arena.get(param.type_annotation) {
                    if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        self.arena
                            .get_type_ref(type_node)
                            .and_then(|type_ref| self.arena.get_identifier_text(type_ref.type_name))
                            .map(std::string::ToString::to_string)
                    } else {
                        None
                    }
                } else {
                    None
                };
            let Some(param_type_name) = param_type_name else {
                continue;
            };
            let Some(type_params) = func.type_parameters.as_ref() else {
                continue;
            };
            for &type_param_idx in &type_params.nodes {
                let Some(type_param_node) = self.arena.get(type_param_idx) else {
                    continue;
                };
                let Some(type_param) = self.arena.get_type_parameter(type_param_node) else {
                    continue;
                };
                let Some(type_param_name) = self.arena.get_identifier_text(type_param.name) else {
                    continue;
                };
                if type_param_name != param_type_name || !type_param.constraint.is_some() {
                    continue;
                }
                self.collect_key_names_from_constraint_node(type_param.constraint, labels);
            }
        }
    }

    fn node_contains(&self, container: NodeIndex, target: NodeIndex) -> bool {
        let Some(container_node) = self.arena.get(container) else {
            return false;
        };
        let Some(target_node) = self.arena.get(target) else {
            return false;
        };
        container_node.pos <= target_node.pos && container_node.end >= target_node.end
    }

    fn collect_key_names_from_constraint_node(
        &self,
        constraint_idx: NodeIndex,
        labels: &mut FxHashSet<String>,
    ) {
        let Some(node) = self.arena.get(constraint_idx) else {
            return;
        };
        if node.kind != syntax_kind_ext::TYPE_OPERATOR {
            return;
        }
        let Some(type_operator) = self.arena.get_type_operator(node) else {
            return;
        };
        if type_operator.operator != SyntaxKind::KeyOfKeyword as u16 {
            return;
        }
        self.collect_key_names_from_type_node(type_operator.type_node, labels);
    }

    fn collect_key_names_from_type_node(
        &self,
        type_node_idx: NodeIndex,
        labels: &mut FxHashSet<String>,
    ) {
        let Some(type_node) = self.arena.get(type_node_idx) else {
            return;
        };
        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = self.arena.get_type_ref(type_node) else {
                    return;
                };
                let Some(type_name) = self.arena.get_identifier_text(type_ref.type_name) else {
                    return;
                };
                self.collect_key_names_from_named_type(type_name, labels);
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                let Some(type_lit) = self.arena.get_type_literal(type_node) else {
                    return;
                };
                for &member_idx in &type_lit.members.nodes {
                    self.collect_key_name_from_member(member_idx, labels);
                }
            }
            _ => {}
        }
    }

    fn collect_key_names_from_named_type(&self, type_name: &str, labels: &mut FxHashSet<String>) {
        let Some(symbol_id) = self.binder.file_locals.get(type_name) else {
            return;
        };
        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            match decl_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    let Some(iface) = self.arena.get_interface(decl_node) else {
                        continue;
                    };
                    for &member_idx in &iface.members.nodes {
                        self.collect_key_name_from_member(member_idx, labels);
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    let Some(alias) = self.arena.get_type_alias(decl_node) else {
                        continue;
                    };
                    self.collect_key_names_from_type_node(alias.type_node, labels);
                }
                _ => {}
            }
        }
    }

    fn collect_key_name_from_member(&self, member_idx: NodeIndex, labels: &mut FxHashSet<String>) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };
        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE
                || k == syntax_kind_ext::METHOD_SIGNATURE =>
            {
                if let Some(signature) = self.arena.get_signature(member_node)
                    && let Some(name) = self.arena.get_identifier_text(signature.name)
                {
                    labels.insert(name.to_string());
                }
            }
            _ => {}
        }
    }
}
