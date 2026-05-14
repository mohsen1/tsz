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

pub(in crate::declaration_emitter) struct CallableDeclParts<'b> {
    pub(in crate::declaration_emitter) modifiers: Option<&'b NodeList>,
    pub(in crate::declaration_emitter) type_parameters: Option<&'b NodeList>,
    pub(in crate::declaration_emitter) parameters: &'b NodeList,
    pub(in crate::declaration_emitter) type_annotation: NodeIndex,
    pub(in crate::declaration_emitter) body: NodeIndex,
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn synthetic_class_extends_alias_source_type_text(
        &self,
        heritage: Option<&NodeList>,
    ) -> Option<String> {
        let heritage = heritage?;
        let (_, expr_idx) = self.non_nameable_extends_heritage_type(heritage)?;
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;
        for arg_idx in arguments.nodes.iter().copied() {
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
                && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            {
                continue;
            }
            if let Some(type_text) =
                self.function_returned_local_class_constructor_type_text(arg_idx)
            {
                return Some(type_text);
            }
        }

        if let Some(text) = self.mixin_call_intersection_source_text(expr_idx) {
            return Some(text);
        }

        self.call_expression_returned_local_class_constructor_text(expr_idx, true)
    }

    /// Recover the source-side return type for a heritage call like
    /// `Mix(A, B)` where `Mix` is a generic function declared with the
    /// signature `<T1, T2, …>(p1: T1, p2: T2, …): T1 & T2 & …`. tsc
    /// computes `T1 & T2 & …` after inferring `Ti = typeof argi`,
    /// producing an intersection synthetic-base alias. Tsz's heritage
    /// inference path collapses this to just the last `Ti`, so synthesize
    /// the intersection text directly from the AST: read the callee's
    /// signature, check the intersection-of-bare-type-parameters return
    /// shape, and rebuild it with `typeof argi` substitutions.
    pub(in crate::declaration_emitter) fn mixin_call_intersection_source_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;
        let arg_idxs: Vec<NodeIndex> = arguments.nodes.to_vec();
        if arg_idxs.is_empty() {
            return None;
        }

        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        // Walk every declaration of the callee symbol; only one needs to be
        // a function-like declaration with the recognised intersection-of-
        // type-parameters return.
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let (type_parameters, parameters, return_type) =
                if let Some(func) = self.arena.get_function(decl_node) {
                    (
                        func.type_parameters.as_ref(),
                        &func.parameters,
                        func.type_annotation,
                    )
                } else if let Some(method) = self.arena.get_method_decl(decl_node) {
                    (
                        method.type_parameters.as_ref(),
                        &method.parameters,
                        method.type_annotation,
                    )
                } else {
                    continue;
                };

            // Need at least one type parameter and matching arity.
            let Some(type_params) = type_parameters else {
                continue;
            };
            if type_params.nodes.is_empty() || parameters.nodes.len() != arg_idxs.len() {
                continue;
            }

            // Collect type-parameter names in declaration order.
            let mut type_param_names: Vec<String> = Vec::with_capacity(type_params.nodes.len());
            for &param_idx in &type_params.nodes {
                let Some(param_node) = self.arena.get(param_idx) else {
                    continue;
                };
                let Some(type_param) = self.arena.get_type_parameter(param_node) else {
                    continue;
                };
                let Some(name) = self.get_identifier_text(type_param.name) else {
                    continue;
                };
                type_param_names.push(name);
            }
            if type_param_names.len() != type_params.nodes.len() {
                continue;
            }

            // Each parameter must be annotated as a bare reference to a
            // distinct type parameter, and the parameters must cover the
            // type parameters in order. `<T, U>(t: T, u: U)` qualifies;
            // `<T>(t: T, u: T)` does not.
            let mut param_to_type_param: Vec<usize> = Vec::with_capacity(parameters.nodes.len());
            for &param_idx in &parameters.nodes {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let annotation = self.arena.get(param.type_annotation)?;
                if annotation.kind != syntax_kind_ext::TYPE_REFERENCE {
                    return None;
                }
                let type_ref = self.arena.get_type_ref(annotation)?;
                if type_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|ta| !ta.nodes.is_empty())
                {
                    return None;
                }
                let name = self.get_identifier_text(type_ref.type_name)?;
                let idx = type_param_names.iter().position(|n| *n == name)?;
                param_to_type_param.push(idx);
            }
            if param_to_type_param.len() != parameters.nodes.len() {
                continue;
            }

            // Return type must be an intersection — either of bare
            // type-parameter references (covered by the simple
            // `<T1, …, Tn>(p1, …, pn): T1 & … & Tn` mixin shape) or
            // of a mix of type-parameter references and other type
            // expressions (covered by `<T>(t: T): T & (abstract new …)`,
            // a common abstract-mixin shape). For each member we either
            // substitute a type-parameter reference with `typeof argi`,
            // or emit the member's source text verbatim.
            let Some(return_node) = self.arena.get(return_type) else {
                continue;
            };
            if return_node.kind != syntax_kind_ext::INTERSECTION_TYPE {
                continue;
            }
            let Some(inter) = self.arena.get_composite_type(return_node) else {
                continue;
            };
            if inter.types.nodes.is_empty() {
                continue;
            }

            enum ReturnPart {
                TypeParam(usize),
                Verbatim(NodeIndex),
            }
            let mut parts_plan: Vec<ReturnPart> = Vec::with_capacity(inter.types.nodes.len());
            let mut used_type_params: Vec<usize> = Vec::new();
            for &member_idx in &inter.types.nodes {
                let bare_param_idx = (|| {
                    let member_node = self.arena.get(member_idx)?;
                    if member_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                        return None;
                    }
                    let type_ref = self.arena.get_type_ref(member_node)?;
                    if type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|ta| !ta.nodes.is_empty())
                    {
                        return None;
                    }
                    let name = self.get_identifier_text(type_ref.type_name)?;
                    type_param_names.iter().position(|n| *n == name)
                })();
                if let Some(idx) = bare_param_idx {
                    if used_type_params.contains(&idx) {
                        // Same type parameter referenced twice — give up.
                        used_type_params.clear();
                        parts_plan.clear();
                        break;
                    }
                    used_type_params.push(idx);
                    parts_plan.push(ReturnPart::TypeParam(idx));
                } else {
                    parts_plan.push(ReturnPart::Verbatim(member_idx));
                }
            }
            if parts_plan.is_empty() {
                continue;
            }
            // At least one arm must reference a type parameter; otherwise
            // tsz's existing inference is fine and our text-side rewrite
            // shouldn't override it.
            if used_type_params.is_empty() {
                continue;
            }

            let mut parts: Vec<String> = Vec::with_capacity(parts_plan.len());
            for part in &parts_plan {
                match part {
                    ReturnPart::TypeParam(tp_idx) => {
                        let arg_position =
                            param_to_type_param.iter().position(|&i| i == *tp_idx)?;
                        let arg_idx = arg_idxs[arg_position];
                        parts.push(self.direct_value_reference_typeof_text(arg_idx)?);
                    }
                    ReturnPart::Verbatim(member_idx) => {
                        let member_node = self.arena.get(*member_idx)?;
                        let raw = self.get_source_slice(member_node.pos, member_node.end)?;
                        // The parser's `end` can extend past the closing
                        // delimiter into the next significant token (e.g.
                        // the function body's `{`). Trim trailing
                        // whitespace and any leftover open brace so the
                        // source-side text matches the type expression
                        // alone.
                        let trimmed = raw
                            .trim_end_matches(|c: char| c.is_whitespace() || c == '{')
                            .trim();
                        parts.push(trimmed.to_string());
                    }
                }
            }
            if parts.is_empty() {
                continue;
            }
            return Some(parts.join(" & "));
        }

        None
    }

    pub(in crate::declaration_emitter) fn replace_whole_words_in_text(
        text: &str,
        replacements: &[(String, String)],
    ) -> String {
        if replacements.is_empty() {
            return text.to_string();
        }

        let protected_spans = Self::protected_type_text_literal_spans(text);
        let mut protected_idx = 0usize;
        let mut result = String::with_capacity(text.len() + 16);
        let bytes = text.as_bytes();
        let text_len = bytes.len();
        let mut last_copied = 0usize;
        let mut i = 0;
        while i < text_len {
            while protected_idx < protected_spans.len() && protected_spans[protected_idx].1 <= i {
                protected_idx += 1;
            }
            if let Some((start, end)) = protected_spans.get(protected_idx).copied()
                && start <= i
                && i < end
            {
                i = end;
                continue;
            }

            let mut best_match: Option<(&str, usize)> = None;
            for (word, replacement) in replacements {
                let word_bytes = word.as_bytes();
                let word_len = word_bytes.len();
                if word_len == 0 || i + word_len > text_len {
                    continue;
                }
                if &bytes[i..i + word_len] != word_bytes {
                    continue;
                }
                let before_ok = i == 0 || !Self::is_ident_char_in_text(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char_in_text(bytes[i + word_len]);
                let qualified_member = i > 0 && bytes[i - 1] == b'.';
                if !before_ok || !after_ok || qualified_member {
                    continue;
                }
                if best_match.is_none_or(|(_, best_len)| word_len > best_len) {
                    best_match = Some((replacement.as_str(), word_len));
                }
            }

            if let Some((replacement, word_len)) = best_match {
                result.push_str(&text[last_copied..i]);
                result.push_str(replacement);
                i += word_len;
                last_copied = i;
                continue;
            }
            i += 1;
        }
        result.push_str(&text[last_copied..]);
        result
    }

    pub(in crate::declaration_emitter) fn contains_whole_word_in_text(
        text: &str,
        word: &str,
    ) -> bool {
        let bytes = text.as_bytes();
        let word_bytes = word.as_bytes();
        let word_len = word_bytes.len();
        let text_len = bytes.len();
        let protected_spans = Self::protected_type_text_literal_spans(text);
        let mut protected_idx = 0usize;
        let mut i = 0;
        while i < text_len {
            while protected_idx < protected_spans.len() && protected_spans[protected_idx].1 <= i {
                protected_idx += 1;
            }
            if let Some((start, end)) = protected_spans.get(protected_idx).copied()
                && start <= i
                && i < end
            {
                i = end;
                continue;
            }

            if i + word_len <= text_len && &bytes[i..i + word_len] == word_bytes {
                let before_ok = i == 0 || !Self::is_ident_char_in_text(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char_in_text(bytes[i + word_len]);
                let qualified_member = i > 0 && bytes[i - 1] == b'.';
                if before_ok && after_ok && !qualified_member {
                    return true;
                }
            }
            i += 1;
        }
        false
    }

    fn protected_type_text_literal_spans(text: &str) -> Vec<(usize, usize)> {
        fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
            i += 1;
            let mut escaped = false;
            while i < bytes.len() {
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'\\' {
                    escaped = true;
                    i += 1;
                    continue;
                }
                i += 1;
                if bytes[i - 1] == quote {
                    break;
                }
            }
            i
        }

        fn scan_template(bytes: &[u8], start: usize, spans: &mut Vec<(usize, usize)>) -> usize {
            let mut segment_start = start;
            let mut i = start + 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => {
                        i = (i + 2).min(bytes.len());
                    }
                    b'`' => {
                        spans.push((segment_start, i + 1));
                        return i + 1;
                    }
                    b'$' if bytes.get(i + 1) == Some(&b'{') => {
                        spans.push((segment_start, i + 2));
                        i = scan_template_placeholder(bytes, i + 2, spans);
                        segment_start = i.saturating_sub(1);
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            spans.push((segment_start, bytes.len()));
            bytes.len()
        }

        fn scan_template_placeholder(
            bytes: &[u8],
            mut i: usize,
            spans: &mut Vec<(usize, usize)>,
        ) -> usize {
            let mut brace_depth = 1usize;
            while i < bytes.len() {
                match bytes[i] {
                    b'\'' | b'"' => {
                        let end = skip_quoted(bytes, i, bytes[i]);
                        spans.push((i, end));
                        i = end;
                    }
                    b'`' => {
                        i = scan_template(bytes, i, spans);
                    }
                    b'{' => {
                        brace_depth += 1;
                        i += 1;
                    }
                    b'}' => {
                        brace_depth = brace_depth.saturating_sub(1);
                        i += 1;
                        if brace_depth == 0 {
                            return i;
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            i
        }

        let bytes = text.as_bytes();
        let mut spans = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\'' | b'"' => {
                    let end = skip_quoted(bytes, i, bytes[i]);
                    spans.push((i, end));
                    i = end;
                }
                b'`' => {
                    i = scan_template(bytes, i, &mut spans);
                }
                _ => {
                    i += 1;
                }
            }
        }
        spans
    }

    const fn is_ident_char_in_text(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
    }

    pub(in crate::declaration_emitter) fn object_rest_binding_excluded_names(
        &self,
        identifier_idx: NodeIndex,
    ) -> Option<Vec<String>> {
        let sym_id = self.value_reference_symbol(identifier_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let parent_idx = self.arena.parent_of(decl_idx)?;
            let parent_node = self.arena.get(parent_idx)?;
            let binding = self.arena.get_binding_element(parent_node)?;
            if !binding.dot_dot_dot_token || binding.name != decl_idx {
                continue;
            }

            let pattern_idx = self.arena.parent_of(parent_idx)?;
            let pattern_node = self.arena.get(pattern_idx)?;
            let pattern = self.arena.get_binding_pattern(pattern_node)?;
            let mut excluded = Vec::new();
            for &element_idx in &pattern.elements.nodes {
                let Some(element_node) = self.arena.get(element_idx) else {
                    continue;
                };
                let Some(element) = self.arena.get_binding_element(element_node) else {
                    continue;
                };
                if element.dot_dot_dot_token {
                    continue;
                }
                let name_idx = if element.property_name.is_some() {
                    element.property_name
                } else {
                    element.name
                };
                if let Some(name) = self.property_name_text_from_arena(self.arena, name_idx) {
                    excluded.push(name);
                }
            }
            return Some(excluded);
        }

        None
    }

    pub(in crate::declaration_emitter) fn omit_object_type_text_properties(
        type_text: &str,
        excluded_names: &[String],
    ) -> String {
        if !type_text.trim_start().starts_with('{') || excluded_names.is_empty() {
            return type_text.to_string();
        }

        type_text
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !excluded_names.iter().any(|name| {
                    trimmed
                        .strip_prefix(name)
                        .is_some_and(|rest| rest.starts_with(':') || rest.starts_with("?:"))
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn qualify_foreign_exported_names_in_text(
        &self,
        source_arena: &NodeArena,
        source_path: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let Some(current_path) = self.current_file_path.as_deref() else {
            return text.to_string();
        };
        if self.paths_refer_to_same_source_file(current_path, source_path) {
            return text.to_string();
        }

        let rel_path =
            self.strip_ts_extensions(&self.calculate_relative_path(current_path, source_path));
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let target_node = source_arena
                .get_export_decl(stmt_node)
                .and_then(|export| source_arena.get(export.export_clause))
                .unwrap_or(stmt_node);
            let export_name = if let Some(decl) = source_arena.get_interface(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_type_alias(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_class(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_enum(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else {
                None
            }
            .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

            let Some(export_name) = export_name else {
                continue;
            };
            if excluded_names.iter().any(|name| name == &export_name) {
                continue;
            }
            let qualified = format!("import(\"{rel_path}\").{export_name}");
            replacements.push((export_name, qualified));
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    pub(in crate::declaration_emitter) fn enclosing_function_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
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
            if let Some(func) = self.arena.get_function(parent_node) {
                return Some(func);
            }
            current = parent_idx;
        }

        None
    }

    pub(in crate::declaration_emitter) fn scratch_declaration_emitter(
        &self,
    ) -> DeclarationEmitter<'a> {
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
        scratch
    }

    pub(in crate::declaration_emitter) fn declaration_emittable_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
        printed_type_text: &str,
    ) -> String {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);

        if type_id == tsz_solver::types::TypeId::ANY
            && let Some(type_text) = self.data_view_new_expression_type_text(initializer)
        {
            return type_text;
        }

        if self.object_literal_prefers_syntax_type_text(initializer)
            && let Some(type_text) =
                self.rewrite_object_literal_computed_member_type_text(initializer, type_id)
        {
            return self.rewrite_exported_import_equals_type_text(type_text);
        }

        if let Some(typeof_text) =
            self.typeof_prefix_for_value_entity(initializer, true, Some(type_id))
        {
            return self.rewrite_exported_import_equals_type_text(typeof_text);
        }

        if (type_id == tsz_solver::types::TypeId::ANY
            || type_id == tsz_solver::types::TypeId::ERROR)
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            return self.rewrite_exported_import_equals_type_text(type_text);
        }

        if type_id != tsz_solver::types::TypeId::ANY
            && type_id != tsz_solver::types::TypeId::ERROR
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
        {
            if let Some(type_text) = self.preferred_expression_type_text(initializer) {
                let type_text = Self::strip_synthetic_anonymous_object_members(&type_text);
                let type_text = self
                    .expand_portable_mapped_object_text_in_current_context(&type_text)
                    .unwrap_or(type_text);
                let type_text =
                    self.rewrite_call_receiver_default_import_aliases(initializer, type_text);
                return self.rewrite_exported_import_equals_type_text(type_text);
            }
            let type_text = Self::strip_synthetic_anonymous_object_members(printed_type_text);
            let type_text = self
                .expand_portable_mapped_object_text_in_current_context(&type_text)
                .unwrap_or(type_text);
            let type_text =
                self.rewrite_call_receiver_default_import_aliases(initializer, type_text);
            return self.rewrite_exported_import_equals_type_text(type_text);
        }

        if (type_id != tsz_solver::types::TypeId::ANY
            || !self.initializer_is_new_expression(initializer))
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            let type_text = Self::strip_synthetic_anonymous_object_members(&type_text);
            if let Some(expanded) =
                self.expand_portable_mapped_object_text_in_current_context(&type_text)
            {
                return self.rewrite_exported_import_equals_type_text(expanded);
            }
            let type_text = self
                .rewrite_const_assertion_object_index_value_union(initializer, &type_text)
                .unwrap_or(type_text);
            let type_text = self
                .enum_value_index_access_alias_type_text(&type_text)
                .unwrap_or(type_text);
            return self.rewrite_exported_import_equals_type_text(type_text);
        }

        let type_text = Self::strip_synthetic_anonymous_object_members(printed_type_text);
        let type_text = self
            .rewrite_const_assertion_object_index_value_union(initializer, &type_text)
            .unwrap_or(type_text);
        if let Some(expanded) =
            self.expand_portable_mapped_object_text_in_current_context(&type_text)
        {
            return self.rewrite_exported_import_equals_type_text(expanded);
        }
        let type_text = self
            .enum_value_index_access_alias_type_text(&type_text)
            .unwrap_or(type_text);
        self.rewrite_exported_import_equals_type_text(type_text)
    }

    pub(in crate::declaration_emitter) fn rewrite_exported_import_equals_type_text(
        &self,
        type_text: String,
    ) -> String {
        let visible_aliases = self.visible_import_equals_type_alias_rewrites();
        let type_text = visible_aliases
            .into_iter()
            .fold(type_text, |text, (target, alias)| {
                Self::replace_qualified_type_reference_text(&text, &target, &alias)
            });

        let aliases = self.exported_import_equals_type_alias_rewrites();
        if aliases.is_empty() {
            return type_text;
        }

        aliases
            .into_iter()
            .fold(type_text, |text, (alias, target)| {
                Self::replace_qualified_type_reference_text(&text, &alias, &target)
            })
    }

    fn visible_import_equals_type_alias_rewrites(&self) -> Vec<(String, String)> {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return Vec::new();
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return Vec::new();
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return Vec::new();
        };

        let current_namespace_path = self.current_namespace_symbol_path();
        let mut aliases = Vec::new();
        self.collect_visible_import_equals_type_aliases(
            &source_file.statements,
            &mut Vec::new(),
            &current_namespace_path,
            &mut aliases,
        );
        aliases.sort_by_key(|(target, _)| std::cmp::Reverse(target.len()));
        aliases.dedup();
        aliases
    }

    fn current_namespace_symbol_path(&self) -> Vec<String> {
        let (Some(binder), Some(mut current)) = (self.binder, self.enclosing_namespace_symbol)
        else {
            return Vec::new();
        };

        let mut path = Vec::new();
        for _ in 0..20 {
            let Some(symbol) = binder.symbols.get(current) else {
                break;
            };
            if !symbol.escaped_name.starts_with("__") {
                path.push(symbol.escaped_name.clone());
            }
            if !symbol.parent.is_some() {
                break;
            }
            current = symbol.parent;
        }
        path.reverse();
        path
    }

    fn collect_visible_import_equals_type_aliases(
        &self,
        statements: &NodeList,
        namespace_path: &mut Vec<String>,
        current_namespace_path: &[String],
        aliases: &mut Vec<(String, String)>,
    ) {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                self.collect_visible_import_equals_type_aliases_in_module(
                    stmt_node,
                    namespace_path,
                    current_namespace_path,
                    aliases,
                );
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && namespace_path.as_slice() == current_namespace_path
            {
                self.collect_visible_import_equals_type_alias(stmt_idx, aliases);
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                && let Some(clause_node) = self.arena.get(export_decl.export_clause)
            {
                if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    self.collect_visible_import_equals_type_aliases_in_module(
                        clause_node,
                        namespace_path,
                        current_namespace_path,
                        aliases,
                    );
                } else if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    && namespace_path.as_slice() == current_namespace_path
                {
                    self.collect_visible_import_equals_type_alias(
                        export_decl.export_clause,
                        aliases,
                    );
                }
            }
        }
    }

    fn collect_visible_import_equals_type_aliases_in_module(
        &self,
        module_node: &Node,
        namespace_path: &mut Vec<String>,
        current_namespace_path: &[String],
        aliases: &mut Vec<(String, String)>,
    ) {
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };
        let Some(module_name) = self.entity_name_text(module.name) else {
            return;
        };

        let old_len = namespace_path.len();
        namespace_path.extend(module_name.split('.').map(ToString::to_string));

        if current_namespace_path.starts_with(namespace_path.as_slice())
            && let Some(body_node) = self.arena.get(module.body)
        {
            if self.arena.get_module(body_node).is_some() {
                self.collect_visible_import_equals_type_aliases_in_module(
                    body_node,
                    namespace_path,
                    current_namespace_path,
                    aliases,
                );
            } else if let Some(block) = self.arena.get_module_block(body_node)
                && let Some(statements) = block.statements.as_ref()
            {
                self.collect_visible_import_equals_type_aliases(
                    statements,
                    namespace_path,
                    current_namespace_path,
                    aliases,
                );
            }
        }

        namespace_path.truncate(old_len);
    }

    fn collect_visible_import_equals_type_alias(
        &self,
        import_idx: NodeIndex,
        aliases: &mut Vec<(String, String)>,
    ) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_decl) = self.arena.get_import_decl(import_node) else {
            return;
        };
        let Some(alias_name) = self.get_identifier_text(import_decl.import_clause) else {
            return;
        };
        let Some(target_text) = self.entity_name_text(import_decl.module_specifier) else {
            return;
        };
        if target_text == alias_name
            || self
                .arena
                .get(import_decl.module_specifier)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
        {
            return;
        }

        aliases.push((target_text, alias_name));
    }

    fn exported_import_equals_type_alias_rewrites(&self) -> Vec<(String, String)> {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return Vec::new();
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return Vec::new();
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return Vec::new();
        };

        let mut aliases = Vec::new();
        self.collect_exported_import_equals_type_aliases(
            &source_file.statements,
            &mut Vec::new(),
            &mut aliases,
        );
        aliases.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.len()));
        aliases.dedup();
        aliases
    }

    fn collect_exported_import_equals_type_aliases(
        &self,
        statements: &NodeList,
        namespace_path: &mut Vec<String>,
        aliases: &mut Vec<(String, String)>,
    ) {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                self.collect_exported_import_equals_type_aliases_in_module(
                    stmt_node,
                    namespace_path,
                    aliases,
                );
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                self.collect_exported_import_equals_type_alias(
                    stmt_idx,
                    namespace_path,
                    aliases,
                    false,
                );
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                && let Some(clause_node) = self.arena.get(export_decl.export_clause)
            {
                if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    self.collect_exported_import_equals_type_aliases_in_module(
                        clause_node,
                        namespace_path,
                        aliases,
                    );
                } else if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    self.collect_exported_import_equals_type_alias(
                        export_decl.export_clause,
                        namespace_path,
                        aliases,
                        true,
                    );
                }
            }
        }
    }

    fn collect_exported_import_equals_type_aliases_in_module(
        &self,
        module_node: &Node,
        namespace_path: &mut Vec<String>,
        aliases: &mut Vec<(String, String)>,
    ) {
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };
        let Some(module_name) = self.entity_name_text(module.name) else {
            return;
        };

        let old_len = namespace_path.len();
        namespace_path.extend(module_name.split('.').map(ToString::to_string));

        if let Some(body_node) = self.arena.get(module.body) {
            if self.arena.get_module(body_node).is_some() {
                self.collect_exported_import_equals_type_aliases_in_module(
                    body_node,
                    namespace_path,
                    aliases,
                );
            } else if let Some(block) = self.arena.get_module_block(body_node)
                && let Some(statements) = block.statements.as_ref()
            {
                self.collect_exported_import_equals_type_aliases(
                    statements,
                    namespace_path,
                    aliases,
                );
            }
        }

        namespace_path.truncate(old_len);
    }

    fn collect_exported_import_equals_type_alias(
        &self,
        import_idx: NodeIndex,
        namespace_path: &[String],
        aliases: &mut Vec<(String, String)>,
        already_exported: bool,
    ) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_decl) = self.arena.get_import_decl(import_node) else {
            return;
        };
        if !already_exported
            && !self
                .arena
                .has_modifier(&import_decl.modifiers, SyntaxKind::ExportKeyword)
        {
            return;
        }
        let Some(alias_name) = self.get_identifier_text(import_decl.import_clause) else {
            return;
        };
        let Some(target_text) = self.entity_name_text(import_decl.module_specifier) else {
            return;
        };
        if target_text == alias_name
            || self
                .arena
                .get(import_decl.module_specifier)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
        {
            return;
        }

        // Top-level exported import aliases (`export import xc = x.c;` at the
        // file root) are always in scope wherever the d.ts is consumed, and
        // tsc prefers the alias spelling over the qualified target. Only
        // namespace-local aliases need a target rewrite — when an outer scope
        // references them, the alias name is not in scope, so the printer's
        // qualified path (`m2.m3.c`) must canonicalize back to its target
        // (`x.c`). Skipping the top-level case prevents the rewrite from
        // clobbering a printer output of `xc` with the longer `x.c`.
        if namespace_path.is_empty() {
            return;
        }
        let alias_text = format!("{}.{}", namespace_path.join("."), alias_name);
        aliases.push((alias_text, target_text));
    }

    fn entity_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(idx);
        }
        if let Some(qualified) = self.arena.get_qualified_name(node) {
            let left = self.entity_name_text(qualified.left)?;
            let right = self.entity_name_text(qualified.right)?;
            return Some(format!("{left}.{right}"));
        }
        if let Some(access) = self.arena.get_access_expr(node) {
            let left = self.entity_name_text(access.expression)?;
            let right = self.entity_name_text(access.name_or_argument)?;
            return Some(format!("{left}.{right}"));
        }
        None
    }

    fn replace_qualified_type_reference_text(type_text: &str, from: &str, to: &str) -> String {
        let mut out = String::with_capacity(type_text.len());
        let mut search_start = 0;

        while let Some(relative_idx) = type_text[search_start..].find(from) {
            let start = search_start + relative_idx;
            let end = start + from.len();
            out.push_str(&type_text[search_start..start]);
            if Self::is_qualified_type_reference_boundary(type_text, start, end) {
                out.push_str(to);
            } else {
                out.push_str(from);
            }
            search_start = end;
        }

        out.push_str(&type_text[search_start..]);
        out
    }

    fn is_qualified_type_reference_boundary(type_text: &str, start: usize, end: usize) -> bool {
        let before = type_text[..start].chars().next_back();
        let after = type_text[end..].chars().next();
        !before.is_some_and(Self::is_qualified_type_reference_part)
            && !after.is_some_and(Self::is_qualified_type_reference_part)
    }

    const fn is_qualified_type_reference_part(ch: char) -> bool {
        ch == '.' || ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    }

    fn enum_value_index_access_alias_type_text(&self, type_text: &str) -> Option<String> {
        let mut inner = type_text.trim();
        let mut array_suffix = String::new();
        while let Some(next) = inner.strip_suffix("[]") {
            array_suffix.push_str("[]");
            inner = next.trim_end();
        }

        let (alias, key_alias) = inner.split_once("[keyof ")?;
        let alias = alias.trim();
        let key_alias = key_alias.strip_suffix(']')?.trim();
        if alias != key_alias || !Self::is_simple_identifier_text(alias) {
            return None;
        }

        let enum_name = self.typeof_enum_alias_target_name(alias)?;
        Some(format!("{enum_name}{array_suffix}"))
    }

    fn typeof_enum_alias_target_name(&self, alias: &str) -> Option<String> {
        let alias_type_node = self.find_local_type_alias_type_node(alias)?;
        let alias_type = self.arena.get(alias_type_node)?;
        if alias_type.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let query = self.arena.get_type_query(alias_type)?;
        let enum_name = self.type_reference_name_text(query.expr_name)?;
        self.local_enum_declaration_exists(&enum_name)
            .then_some(enum_name)
    }

    fn local_enum_declaration_exists(&self, name: &str) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder
            .file_locals
            .get(name)
            .or_else(|| binder.current_scope.get(name))
        else {
            return false;
        };
        let Some(symbol_data) = binder.symbols.get(symbol) else {
            return false;
        };
        symbol_data.declarations.iter().copied().any(|decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| self.arena.get_enum(node).is_some())
        })
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
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(expr_idx)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_declaration_symbol(raw_sym_id, binder));

        self.declared_type_annotation_text_for_symbol(sym_id)
            .or_else(|| self.property_access_declared_type_annotation_text(expr_idx))
    }

    pub(in crate::declaration_emitter) fn value_reference_symbol_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let cache = self.type_cache.as_ref()?;
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        let symbol = binder.symbols.get(resolved_sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };

            if let Some(prop_decl) = self.arena.get_property_decl(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, prop_decl.name])
            {
                let effective_type = if self
                    .arena
                    .has_modifier(&prop_decl.modifiers, SyntaxKind::ReadonlyKeyword)
                {
                    type_id
                } else {
                    self.type_interner
                        .map(|interner| {
                            tsz_solver::operations::widening::widen_literal_type(interner, type_id)
                        })
                        .unwrap_or(type_id)
                };
                return Some(self.print_type_id(effective_type));
            }

            if let Some(accessor) = self.arena.get_accessor(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, accessor.name])
            {
                return Some(self.print_type_id(type_id));
            }
        }

        let type_id = cache.symbol_types.get(&resolved_sym_id).copied()?;
        Some(self.print_type_id(type_id))
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
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(call.expression)?;
        let imported_module = self
            .imported_value_module_specifier(raw_sym_id, binder)
            .or_else(|| self.imported_value_module_specifier_from_syntax(call.expression));
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .or_else(|| {
                imported_module.as_deref().and_then(|module_specifier| {
                    self.imported_value_export_symbol_from_syntax(
                        call.expression,
                        module_specifier,
                        binder,
                    )
                })
            })
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            let source_file = self.arena_source_file(source_arena)?;
            let is_ambient_function =
                source_file.is_declaration_file || source_arena.is_declare_ref(callable.modifiers);
            let is_source_overload_signature = callable.body.is_none()
                && callable
                    .type_parameters
                    .is_some_and(|params| !params.nodes.is_empty());
            let is_source_with_return_annotation =
                callable.body.is_some() && callable.type_annotation.is_some();
            if imported_module.is_some()
                && !is_ambient_function
                && self
                    .current_file_path
                    .as_deref()
                    .is_some_and(|current_path| {
                        self.paths_refer_to_same_source_file(current_path, &source_file.file_name)
                    })
            {
                return None;
            }
            if (!is_ambient_function
                && !is_source_overload_signature
                && !is_source_with_return_annotation)
                || !callable.type_annotation.is_some()
                || !self.function_signature_accepts_call_arguments(
                    source_arena,
                    callable.parameters,
                    call,
                )
            {
                return None;
            }

            let mut type_text = self
                .source_slice_from_arena(source_arena, callable.type_annotation)
                .or_else(|| {
                    self.emit_type_node_text_from_arena(source_arena, callable.type_annotation)
                })?
                .trim_end()
                .trim_end_matches(';')
                .trim_end()
                .to_string();

            let mut type_param_names = Vec::new();
            let mut type_param_substitutions = Vec::new();
            let mut type_param_constraints = Vec::new();
            let mut type_param_fallbacks = Vec::new();
            if let Some(type_params) = callable.type_parameters {
                for &param_idx in &type_params.nodes {
                    if let Some(param_node) = source_arena.get(param_idx)
                        && let Some(param) = source_arena.get_type_parameter(param_node)
                        && let Some(name_text) =
                            self.identifier_text_from_arena(source_arena, param.name)
                    {
                        let fallback = if param.default.is_some() {
                            self.emit_type_node_text_from_arena(source_arena, param.default)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.default)
                                })
                        } else if param.constraint.is_some() {
                            self.emit_type_node_text_from_arena(source_arena, param.constraint)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.constraint)
                                })
                        } else {
                            None
                        };
                        if param.constraint.is_some()
                            && let Some(constraint) = self
                                .emit_type_node_text_from_arena(source_arena, param.constraint)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.constraint)
                                })
                        {
                            type_param_constraints.push((name_text.clone(), constraint));
                        }
                        if let Some(fallback) = fallback {
                            type_param_fallbacks.push((name_text.clone(), fallback));
                        }
                        type_param_names.push(name_text);
                    }
                }

                if !explicit_type_args.is_empty() {
                    for (name_text, arg_text) in
                        type_param_names.iter().zip(explicit_type_args.iter())
                    {
                        type_param_substitutions.push((name_text.clone(), arg_text.clone()));
                    }
                } else {
                    type_param_substitutions.extend(
                        self.infer_call_type_param_substitutions_from_arguments(
                            source_arena,
                            callable.parameters,
                            call,
                            &type_param_names,
                            &type_param_constraints,
                        ),
                    );
                }
                if Self::type_text_contains_mapped_type_literal(&type_text) {
                    self.preserve_literal_mapped_return_type_substitutions(
                        source_arena,
                        callable.parameters,
                        call,
                        &type_param_names,
                        &mut type_param_substitutions,
                    );
                }
            }
            for (name_text, fallback_text) in &type_param_fallbacks {
                if type_param_substitutions
                    .iter()
                    .any(|(substituted, _)| substituted == name_text)
                    || !Self::contains_whole_word_in_text(&type_text, name_text)
                {
                    continue;
                }
                let fallback_text =
                    Self::replace_whole_words_in_text(fallback_text, &type_param_substitutions);
                type_param_substitutions.push((name_text.clone(), fallback_text));
            }
            if explicit_type_args.is_empty()
                && type_param_substitutions.is_empty()
                && type_param_names
                    .iter()
                    .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                return None;
            }
            let mut protected_type_param_names = Vec::new();
            let protected_substitutions = type_param_substitutions
                .iter()
                .enumerate()
                .map(|(substitution_idx, (name_text, arg_text))| {
                    let mut protected_arg_text = arg_text.clone();
                    for (param_idx, param_name) in type_param_names.iter().enumerate() {
                        if !Self::contains_whole_word_in_text(&protected_arg_text, param_name) {
                            continue;
                        }
                        let protected_name =
                            format!("__tszDeclEmitTypeParam{substitution_idx}_{param_idx}__");
                        protected_arg_text = Self::replace_whole_words_in_text(
                            &protected_arg_text,
                            &[(param_name.clone(), protected_name.clone())],
                        );
                        protected_type_param_names.push((protected_name, param_name.clone()));
                    }
                    (name_text.clone(), protected_arg_text)
                })
                .collect::<Vec<_>>();
            type_text = Self::replace_whole_words_in_text(&type_text, &protected_substitutions);
            if type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                return None;
            }
            for (protected_name, param_name) in protected_type_param_names {
                type_text = type_text.replace(&protected_name, &param_name);
            }
            if Self::leading_type_reference_name(&type_text)
                .is_some_and(Self::is_builtin_conditional_utility_type_name)
                && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
            {
                return Some(self.print_type_id_expanded_for_inferred_declaration(type_id));
            }
            if let Some(expanded) =
                self.event_like_correlated_alias_return_text(source_arena, &type_text, call)
            {
                type_text = expanded;
            } else if let Some(expanded) =
                Self::expand_tuple_item_lookup_mapped_type_text(&type_text)
            {
                type_text = expanded;
            }

            let source_path = self.get_symbol_source_path(sym_id, binder).or_else(|| {
                self.arena_to_path
                    .get(&(source_arena as *const NodeArena as usize))
                    .cloned()
            });
            type_text = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            if let (Some(source_path), Some(module_specifier)) =
                (source_path.as_deref(), imported_module.as_deref())
                && let Some(rewritten) = self.rewrite_typeof_import_default_return_type(
                    source_path,
                    module_specifier,
                    &type_text,
                    binder,
                )
            {
                type_text = rewritten;
            }
            if let Some(module_specifier) = imported_module.as_deref() {
                type_text = self.qualify_ambient_module_exported_names_in_text(
                    source_arena,
                    module_specifier,
                    &type_text,
                    &type_param_names,
                );
                if !Self::type_text_contains_import_type(&type_text)
                    && let Some(root_name) = Self::leading_type_reference_name(&type_text)
                    && !type_param_names.iter().any(|name| name == root_name)
                    && self.imported_module_exports_name(binder, module_specifier, root_name)
                {
                    type_text = format!(
                        "import(\"{module_specifier}\").{}{}",
                        root_name,
                        &type_text[root_name.len()..]
                    );
                }
            }
            if let Some(source_path) = source_path.as_deref() {
                if !Self::type_text_contains_import_type(&type_text) {
                    type_text = self.qualify_foreign_exported_names_in_text(
                        source_arena,
                        source_path,
                        &type_text,
                        &type_param_names,
                    );
                }
                if self
                    .current_file_path
                    .as_deref()
                    .is_some_and(|current_path| {
                        !self.paths_refer_to_same_source_file(current_path, source_path)
                            && type_text.starts_with("typeof ")
                            && !Self::type_text_contains_import_type(&type_text)
                    })
                {
                    return None;
                }
                if self.type_text_contains_unqualified_foreign_value_export(
                    source_arena,
                    source_path,
                    &type_text,
                ) {
                    return None;
                }
            }
            if let (Some(source_path), Some(module_specifier)) =
                (source_path.as_deref(), imported_module.as_deref())
                && self.package_json_name_matches_import_specifier(source_path, module_specifier)
            {
                type_text =
                    Self::rewrite_relative_import_type_specifiers(&type_text, module_specifier);
            }
            type_text = Self::ensure_single_line_type_literal_member_semicolon(&type_text);
            let formatted = self.format_reused_call_structural_return_type_text(&type_text);
            Some(
                self.expand_rest_tuple_parameters_in_function_type_text(expr_idx, &formatted)
                    .unwrap_or(formatted),
            )
        })
    }

    fn format_reused_call_structural_return_type_text(&self, type_text: &str) -> String {
        if !type_text.contains(" & ") || !type_text.contains("=> {") {
            return type_text.to_string();
        }

        let mut out = String::with_capacity(type_text.len() + 16);
        let mut rest = type_text;
        let member_indent = "    ".repeat((self.indent_level + 1) as usize);
        let closing_indent = "    ".repeat(self.indent_level as usize);

        while let Some(start) = rest.find("=> {") {
            let (before, after_marker) = rest.split_at(start + 4);
            out.push_str(before);
            let Some(end) = after_marker.find('}') else {
                out.push_str(after_marker);
                return out;
            };
            let body = after_marker[..end].trim();
            if body.is_empty()
                || body.contains('\n')
                || body.contains(';')
                || body.contains(',')
                || !body.contains(':')
            {
                out.push_str(&after_marker[..=end]);
                rest = &after_marker[end + 1..];
                continue;
            }

            let member = body.trim_end_matches(';').trim();
            out.push('\n');
            out.push_str(&member_indent);
            out.push_str(member);
            out.push(';');
            out.push('\n');
            out.push_str(&closing_indent);
            out.push('}');
            rest = &after_marker[end + 1..];
        }

        out.push_str(rest);
        out
    }

    fn preserve_literal_mapped_return_type_substitutions(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        call: &tsz_parser::parser::node::CallExprData,
        type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let Some(args) = call.arguments.as_ref() else {
            return;
        };

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
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
            let param_type_text = param_type_text.trim();
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_type_text)
            {
                continue;
            }
            let Some(substitution_text) = self
                .enclosing_parameter_type_annotation_text_for_identifier(arg_idx)
                .or_else(|| self.reference_declared_type_annotation_text(arg_idx))
                .filter(|text| Self::simple_type_reference_name(text).is_some())
                .or_else(|| self.const_literal_initializer_text(arg_idx))
            else {
                continue;
            };
            if let Some((_, existing)) = substitutions
                .iter_mut()
                .find(|(name, _)| name.as_str() == param_type_text)
            {
                *existing = substitution_text;
            } else {
                substitutions.push((param_type_text.to_string(), substitution_text));
            }
        }
    }

    fn enclosing_parameter_type_annotation_text_for_identifier(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let arg_name = self.get_identifier_text(arg_idx)?;
        let mut current = arg_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if let Some(func) = self.arena.get_function(parent_node) {
                for &param_idx in &func.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    if self.get_identifier_text(param.name).as_deref() == Some(arg_name.as_str()) {
                        return self
                            .type_annotation_text_from_arena_node(self.arena, param.type_annotation)
                            .or_else(|| {
                                self.source_slice_from_arena(self.arena, param.type_annotation)
                            })
                            .map(|text| text.trim().to_string());
                    }
                }
                return None;
            }
            current = parent_idx;
        }
        None
    }

    fn ensure_single_line_type_literal_member_semicolon(type_text: &str) -> String {
        let trimmed = type_text.trim();
        if trimmed.contains('\n') {
            return type_text.to_string();
        }
        let Some(inner) = trimmed
            .strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
            .map(str::trim)
        else {
            return type_text.to_string();
        };
        if inner.is_empty() || inner.ends_with(';') || inner.contains(';') || !inner.contains(':') {
            type_text.to_string()
        } else {
            format!("{{ {inner}; }}")
        }
    }

    pub(in crate::declaration_emitter) fn imported_static_method_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let callee_node = self.arena.get(call.expression)?;
        let access = self.arena.get_access_expr(callee_node)?;
        let receiver_name = self.get_identifier_text(access.expression)?;
        let method_name = self.get_identifier_text(access.name_or_argument)?;
        let imported_module =
            self.imported_value_module_specifier_from_syntax(access.expression)?;
        if imported_module.starts_with('.') || imported_module.starts_with('/') {
            return None;
        }

        let binder = self.binder?;
        let imported_name = self
            .imported_value_export_name_from_syntax(access.expression, &imported_module)
            .unwrap_or(receiver_name);
        let class_sym = self
            .export_symbol_from_module_specifier(binder, &imported_module, &imported_name)
            .or_else(|| {
                self.imported_value_export_symbol_from_syntax(
                    access.expression,
                    &imported_module,
                    binder,
                )
            })
            .or_else(|| {
                let raw_sym_id = self.value_reference_symbol(access.expression)?;
                self.resolve_portability_import_alias(raw_sym_id, binder)
                    .or_else(|| Some(self.resolve_portability_symbol(raw_sym_id, binder)))
            })?;
        let class_sym = self.resolve_portability_symbol(class_sym, binder);
        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());

        let from_symbol = self.with_symbol_declarations(class_sym, |source_arena, decl_idx| {
            let class_decl = Self::class_decl_from_symbol_decl(source_arena, decl_idx)?;
            self.imported_static_method_return_type_from_class_decl(
                binder,
                source_arena,
                class_decl,
                &imported_module,
                &imported_name,
                &method_name,
                call,
                &explicit_type_args,
            )
        });
        from_symbol.or_else(|| {
            self.imported_static_method_return_type_from_named_classes(
                binder,
                &imported_module,
                &imported_name,
                &method_name,
                call,
                &explicit_type_args,
            )
        })
    }

    fn imported_static_method_return_type_from_named_classes(
        &self,
        binder: &BinderState,
        imported_module: &str,
        imported_name: &str,
        method_name: &str,
        call: &tsz_parser::parser::node::CallExprData,
        explicit_type_args: &[String],
    ) -> Option<String> {
        for symbol in binder.symbols.iter() {
            if symbol.escaped_name != imported_name {
                continue;
            }
            let Some(source_arena) = binder
                .symbol_arenas
                .get(&symbol.id)
                .or_else(|| self.global_symbol_arenas.get(&symbol.id))
                .map(|arena| arena.as_ref())
            else {
                continue;
            };
            for decl_idx in symbol.declarations.iter().copied() {
                let Some(class_decl) = Self::class_decl_from_symbol_decl(source_arena, decl_idx)
                else {
                    continue;
                };
                if let Some(type_text) = self.imported_static_method_return_type_from_class_decl(
                    binder,
                    source_arena,
                    class_decl,
                    imported_module,
                    imported_name,
                    method_name,
                    call,
                    explicit_type_args,
                ) {
                    return Some(type_text);
                }
            }
        }

        None
    }

    #[allow(clippy::too_many_arguments)]
    fn imported_static_method_return_type_from_class_decl(
        &self,
        binder: &BinderState,
        source_arena: &NodeArena,
        class_decl: &tsz_parser::parser::node::ClassData,
        imported_module: &str,
        imported_name: &str,
        method_name: &str,
        call: &tsz_parser::parser::node::CallExprData,
        explicit_type_args: &[String],
    ) -> Option<String> {
        for &member_idx in &class_decl.members.nodes {
            let Some(member_node) = source_arena.get(member_idx) else {
                continue;
            };
            let Some(func) = source_arena.get_method_decl(member_node) else {
                continue;
            };
            if !source_arena.is_static(&func.modifiers) {
                continue;
            }
            if self
                .identifier_text_from_arena(source_arena, func.name)
                .as_deref()
                != Some(method_name)
            {
                continue;
            }
            if func.type_annotation.is_none()
                || !self.function_signature_accepts_call_arguments(
                    source_arena,
                    &func.parameters,
                    call,
                )
            {
                continue;
            }

            let mut type_text = self
                .emit_type_node_text_from_arena(source_arena, func.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, func.type_annotation))?
                .trim_end()
                .trim_end_matches(';')
                .trim_end()
                .to_string();
            let mut type_param_names = Vec::new();
            let mut type_param_substitutions = Vec::new();
            let mut type_param_fallbacks = Vec::new();
            if let Some(type_params) = func.type_parameters.as_ref() {
                for &param_idx in &type_params.nodes {
                    let Some(param_node) = source_arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = source_arena.get_type_parameter(param_node) else {
                        continue;
                    };
                    let Some(name_text) = self.identifier_text_from_arena(source_arena, param.name)
                    else {
                        continue;
                    };
                    let fallback = if param.default.is_some() {
                        self.emit_type_node_text_from_arena(source_arena, param.default)
                            .or_else(|| self.source_slice_from_arena(source_arena, param.default))
                    } else if param.constraint.is_some() {
                        self.emit_type_node_text_from_arena(source_arena, param.constraint)
                            .or_else(|| {
                                self.source_slice_from_arena(source_arena, param.constraint)
                            })
                    } else {
                        None
                    };
                    if let Some(fallback) = fallback {
                        type_param_fallbacks.push((name_text.clone(), fallback));
                    }
                    type_param_names.push(name_text);
                }
            }
            for (name_text, arg_text) in type_param_names.iter().zip(explicit_type_args.iter()) {
                type_param_substitutions.push((name_text.clone(), arg_text.clone()));
            }
            for (name_text, fallback_text) in &type_param_fallbacks {
                if type_param_substitutions
                    .iter()
                    .any(|(substituted, _)| substituted == name_text)
                    || !Self::contains_whole_word_in_text(&type_text, name_text)
                {
                    continue;
                }
                let fallback_text =
                    Self::replace_whole_words_in_text(fallback_text, &type_param_substitutions);
                type_param_substitutions.push((name_text.clone(), fallback_text));
            }
            type_text = Self::replace_whole_words_in_text(&type_text, &type_param_substitutions);
            if type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                continue;
            }

            let excluded_names = [imported_name.to_string()];
            return Some(self.qualify_public_package_names_in_text(
                binder,
                imported_module,
                &type_text,
                &excluded_names,
            ));
        }

        None
    }

    fn class_decl_from_symbol_decl(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::ClassData> {
        let class_idx = Self::class_decl_index_from_symbol_decl(arena, decl_idx)?;
        let node = arena.get(class_idx)?;
        arena.get_class(node)
    }

    pub(in crate::declaration_emitter) fn class_decl_index_from_symbol_decl(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = arena.get(current)?;
            if arena.get_class(node).is_some() {
                return Some(current);
            }
            current = arena.parent_of(current)?;
        }

        None
    }

    pub(in crate::declaration_emitter) fn imported_value_module_specifier(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        self.import_symbol_map
            .get(&sym_id)
            .cloned()
            .or_else(|| binder.symbols.get(sym_id)?.import_module.clone())
    }

    pub(in crate::declaration_emitter) fn imported_value_module_specifier_from_syntax(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let local_name = self.get_identifier_text(expr_idx)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && self.get_identifier_text(clause.name).as_deref() == Some(local_name.as_str())
            {
                return Some(module_lit.text.clone());
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some()
                    && self.get_identifier_text(bindings.name).as_deref()
                        == Some(local_name.as_str())
                {
                    return Some(module_lit.text.clone());
                }
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref()
                        == Some(local_name.as_str())
                    {
                        return Some(module_lit.text.clone());
                    }
                }
            }
        }

        None
    }

    fn imported_value_export_symbol_from_syntax(
        &self,
        expr_idx: NodeIndex,
        module_specifier: &str,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let export_name =
            self.imported_value_export_name_from_syntax(expr_idx, module_specifier)?;
        if let Some(sym_id) = binder
            .module_exports
            .get(module_specifier)
            .and_then(|exports| exports.get(&export_name))
        {
            return Some(sym_id);
        }

        let module_paths = if module_specifier.starts_with('.') || module_specifier.starts_with('/')
        {
            let current_path = self.current_file_path.as_deref()?;
            self.matching_module_export_paths(binder, current_path, module_specifier)
        } else {
            let mut paths: Vec<_> = binder
                .module_exports
                .keys()
                .filter_map(|module_path| {
                    (self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                        || self.node_modules_package_path_matches_import_specifier(
                            module_path,
                            module_specifier,
                        )
                        || self.node_modules_package_contains_import_specifier(
                            module_path,
                            module_specifier,
                        )
                        || self.package_json_name_matches_import_specifier(
                            module_path,
                            module_specifier,
                        ))
                    .then_some(module_path.as_str())
                })
                .collect();
            paths.sort();
            paths
        };
        for module_path in module_paths {
            if let Some(sym_id) = binder
                .module_exports
                .get(module_path)
                .and_then(|exports| exports.get(&export_name))
            {
                return Some(sym_id);
            }
        }

        None
    }

    fn imported_value_export_name_from_syntax(
        &self,
        expr_idx: NodeIndex,
        module_specifier: &str,
    ) -> Option<String> {
        let local_name = self.get_identifier_text(expr_idx)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            if module_lit.text != module_specifier {
                continue;
            }

            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && self.get_identifier_text(clause.name).as_deref() == Some(local_name.as_str())
            {
                return Some("default".to_string());
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some() {
                    continue;
                }
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref()
                        != Some(local_name.as_str())
                    {
                        continue;
                    }
                    return self
                        .get_identifier_text(specifier.property_name)
                        .or_else(|| self.get_identifier_text(specifier.name));
                }
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn node_modules_package_path_matches_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(module_path).components().collect();
        let Some(nm_idx) = components.iter().position(|component| {
            matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
        }) else {
            return false;
        };

        let pkg_start = nm_idx + 1;
        if components.len() == pkg_start + 1
            && let Component::Normal(part) = components[pkg_start]
            && let Some(file_name) = part.to_str()
            && let Some(runtime_path) = self.declaration_runtime_relative_path(file_name)
        {
            let runtime_path = runtime_path.trim_start_matches("./");
            let package_name = runtime_path
                .strip_suffix(".js")
                .unwrap_or(runtime_path)
                .trim_end_matches("/index");
            return module_specifier == package_name;
        }

        let pkg_len = if components.get(pkg_start).is_some_and(|component| {
            matches!(component, Component::Normal(part) if part.to_str().is_some_and(|text| text.starts_with('@')))
        }) {
            2
        } else {
            1
        };
        if components.len() < pkg_start + pkg_len {
            return false;
        }

        let package_name = components[pkg_start..pkg_start + pkg_len]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        let relative_path = components[pkg_start + pkg_len..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        let Some(runtime_subpath) = self.declaration_runtime_relative_path(&relative_path) else {
            return false;
        };
        let mut runtime_subpath = runtime_subpath.trim_start_matches("./").to_string();
        if runtime_subpath.ends_with("/index.js") {
            runtime_subpath.truncate(runtime_subpath.len() - "/index.js".len());
        } else if runtime_subpath == "index.js" {
            runtime_subpath.clear();
        }

        if runtime_subpath.is_empty() {
            module_specifier == package_name
        } else {
            module_specifier == format!("{package_name}/{runtime_subpath}")
        }
    }

    pub(in crate::declaration_emitter) fn imported_module_exports_name(
        &self,
        binder: &BinderState,
        module_specifier: &str,
        export_name: &str,
    ) -> bool {
        if binder
            .module_exports
            .get(module_specifier)
            .is_some_and(|exports| exports.get(export_name).is_some())
        {
            return true;
        }

        if let Some(current_path) = self.current_file_path.as_deref() {
            for module_path in
                self.matching_module_export_paths(binder, current_path, module_specifier)
            {
                if binder
                    .module_exports
                    .get(module_path)
                    .is_some_and(|exports| exports.get(export_name).is_some())
                {
                    return true;
                }
            }
        }

        if !module_specifier.starts_with('.') && !module_specifier.starts_with('/') {
            return binder.module_exports.iter().any(|(module_path, exports)| {
                (self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                    || self.node_modules_package_path_matches_import_specifier(
                        module_path,
                        module_specifier,
                    )
                    || self.node_modules_package_contains_import_specifier(
                        module_path,
                        module_specifier,
                    ))
                    && exports.get(export_name).is_some()
            });
        }

        false
    }

    pub(in crate::declaration_emitter) fn leading_type_reference_name(
        type_text: &str,
    ) -> Option<&str> {
        let trimmed = type_text.trim_start();
        if Self::type_text_starts_with_import_type(trimmed) || trimmed.starts_with("typeof ") {
            return None;
        }
        let end = trimmed
            .char_indices()
            .find_map(|(idx, ch)| {
                (!(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())).then_some(idx)
            })
            .unwrap_or(trimmed.len());
        if end == 0 {
            return None;
        }
        let name = &trimmed[..end];
        name.chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
            .then_some(name)
    }

    pub(in crate::declaration_emitter) fn type_text_starts_with_string_intrinsic(
        type_text: &str,
    ) -> bool {
        matches!(
            Self::leading_type_reference_name(type_text),
            Some("Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize")
        )
    }

    pub(in crate::declaration_emitter) fn function_signature_accepts_call_arguments(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> bool {
        let arg_count = call.arguments.as_ref().map_or(0, |args| args.nodes.len());
        let mut required_count = 0usize;
        let mut has_rest = false;

        for &param_idx in &parameters.nodes {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            has_rest |= param.dot_dot_dot_token;
            if !param.dot_dot_dot_token
                && !param.question_token
                && param.initializer == NodeIndex::NONE
            {
                required_count += 1;
            }
        }

        arg_count >= required_count && (has_rest || arg_count <= parameters.nodes.len())
    }

    pub(in crate::declaration_emitter) fn callable_decl_parts_from_node<'b>(
        source_arena: &'b NodeArena,
        decl_node: &'b Node,
    ) -> Option<CallableDeclParts<'b>> {
        if let Some(func) = source_arena.get_function(decl_node) {
            return Some(CallableDeclParts {
                modifiers: func.modifiers.as_ref(),
                type_parameters: func.type_parameters.as_ref(),
                parameters: &func.parameters,
                type_annotation: func.type_annotation,
                body: func.body,
            });
        }

        if let Some(method) = source_arena.get_method_decl(decl_node) {
            return Some(CallableDeclParts {
                modifiers: method.modifiers.as_ref(),
                type_parameters: method.type_parameters.as_ref(),
                parameters: &method.parameters,
                type_annotation: method.type_annotation,
                body: method.body,
            });
        }

        None
    }

    fn qualify_ambient_module_exported_names_in_text(
        &self,
        source_arena: &NodeArena,
        module_specifier: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_ambient_module_export_replacements(
                source_arena,
                stmt_idx,
                module_specifier,
                excluded_names,
                &mut replacements,
            );
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    fn collect_ambient_module_export_replacements(
        &self,
        source_arena: &NodeArena,
        module_idx: NodeIndex,
        module_specifier: &str,
        excluded_names: &[String],
        replacements: &mut Vec<(String, String)>,
    ) {
        let Some(module_node) = source_arena.get(module_idx) else {
            return;
        };
        let Some(module) = source_arena.get_module(module_node) else {
            return;
        };

        let Some(name_node) = source_arena.get(module.name) else {
            return;
        };
        if name_node.kind != SyntaxKind::StringLiteral as u16 {
            return;
        }
        let Some(literal) = source_arena.get_literal(name_node) else {
            return;
        };
        if literal.text != module_specifier {
            return;
        }

        let Some(body_node) = source_arena.get(module.body) else {
            return;
        };
        if source_arena.get_module(body_node).is_some() {
            self.collect_ambient_module_export_replacements(
                source_arena,
                module.body,
                module_specifier,
                excluded_names,
                replacements,
            );
            return;
        }

        let Some(block) = source_arena.get_module_block(body_node) else {
            return;
        };
        let Some(statements) = block.statements.as_ref() else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let export_name = if let Some(decl) = source_arena.get_interface(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_type_alias(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_class(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_enum(stmt_node) {
                Some(decl.name)
            } else {
                source_arena.get_function(stmt_node).map(|decl| decl.name)
            }
            .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

            let Some(export_name) = export_name else {
                continue;
            };
            if excluded_names.iter().any(|name| name == &export_name) {
                continue;
            }
            let qualified = format!("import(\"{module_specifier}\").{export_name}");
            replacements.push((export_name, qualified));
        }
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
}

#[cfg(test)]
#[path = "type_inference_tests.rs"]
mod tests;
