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
                        if member_node.kind == syntax_kind_ext::TYPE_LITERAL
                            && let Some(type_text) =
                                self.emit_type_node_text_from_arena(self.arena, *member_idx)
                        {
                            parts.push(type_text.trim().to_string());
                            continue;
                        }
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
                let qualified_member =
                    i > 0 && bytes[i - 1] == b'.' && !Self::word_has_ellipsis_prefix(bytes, i);
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
                let qualified_member =
                    i > 0 && bytes[i - 1] == b'.' && !Self::word_has_ellipsis_prefix(bytes, i);
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

    pub(super) const fn is_ident_char_in_text(b: u8) -> bool {
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

    /// Scratch emitter pre-configured for emitting class members into the body
    /// of an anonymous constructor object type (`{ new(...): { ...members... } }`).
    /// Sets `indent_level` and `in_object_type_class_body = true` so that
    /// property declarations use `: T` annotation form rather than `= value`
    /// initializer form (initializer syntax is not allowed in object type
    /// literals).
    pub(in crate::declaration_emitter) fn scratch_object_type_body_emitter(
        &self,
        indent_level: u32,
    ) -> DeclarationEmitter<'a> {
        let mut scratch = self.scratch_declaration_emitter();
        scratch.indent_level = indent_level;
        scratch.in_object_type_class_body = true;
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
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
        }

        if self.initializer_is_new_expression(initializer)
            && let Some(type_text) = self.construct_return_new_expression_type_text(initializer)
        {
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
        }

        if self.object_literal_prefers_syntax_type_text(initializer)
            && let Some(type_text) =
                self.rewrite_object_literal_computed_member_type_text(initializer, type_id)
        {
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
        }

        if let Some(typeof_text) =
            self.typeof_prefix_for_value_entity(initializer, true, Some(type_id))
        {
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, typeof_text);
        }

        if self
            .arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && let Some(type_text) = self.infer_object_literal_type_text_at(initializer, 0)
        {
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
        }

        let widened = self.widen_unique_symbol_value_type_for_dts(type_id, 0);
        if widened != type_id
            && !self.inferred_declaration_preserves_initializer_type_arguments(initializer, type_id)
        {
            let type_text = self.print_type_id_for_inferred_declaration(widened);
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
        }

        if (type_id == tsz_solver::types::TypeId::ANY
            || type_id == tsz_solver::types::TypeId::ERROR)
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
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
                return self
                    .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
            }
            let type_text = Self::strip_synthetic_anonymous_object_members(printed_type_text);
            let type_text = self
                .expand_portable_mapped_object_text_in_current_context(&type_text)
                .unwrap_or(type_text);
            let type_text =
                self.rewrite_call_receiver_default_import_aliases(initializer, type_text);
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
        }

        if (type_id != tsz_solver::types::TypeId::ANY
            || !self.initializer_is_new_expression(initializer))
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            let type_text = Self::strip_synthetic_anonymous_object_members(&type_text);
            if let Some(expanded) =
                self.expand_portable_mapped_object_text_in_current_context(&type_text)
            {
                return self
                    .rewrite_initializer_exported_import_equals_type_text(initializer, expanded);
            }
            let type_text = self
                .rewrite_const_assertion_object_index_value_union(initializer, &type_text)
                .unwrap_or(type_text);
            let type_text = self
                .enum_value_index_access_alias_type_text(&type_text)
                .unwrap_or(type_text);
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, type_text);
        }

        let type_text = Self::strip_synthetic_anonymous_object_members(printed_type_text);
        let type_text = self
            .rewrite_const_assertion_object_index_value_union(initializer, &type_text)
            .unwrap_or(type_text);
        if let Some(expanded) =
            self.expand_portable_mapped_object_text_in_current_context(&type_text)
        {
            return self
                .rewrite_initializer_exported_import_equals_type_text(initializer, expanded);
        }
        let type_text = self
            .enum_value_index_access_alias_type_text(&type_text)
            .unwrap_or(type_text);
        self.rewrite_initializer_exported_import_equals_type_text(initializer, type_text)
    }

    fn inferred_declaration_preserves_initializer_type_arguments(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        self.arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
            && tsz_solver::visitor::application_id(interner, type_id).is_some()
    }

    pub(in crate::declaration_emitter) fn widened_inferred_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.symbol_has_unique_symbol_type(sym_id)
        {
            return Some("symbol".to_string());
        }
        let type_id = self
            .get_node_type_or_names(&[expr_idx])
            .or_else(|| self.get_type_via_symbol(expr_idx))?;
        self.type_interner?;
        let widened = self.widen_unique_symbol_value_type_for_dts(type_id, 0);
        (widened != type_id).then(|| self.print_type_id_for_inferred_declaration(widened))
    }

    pub(in crate::declaration_emitter) fn widen_unique_symbol_value_type_for_dts(
        &self,
        type_id: tsz_solver::types::TypeId,
        _depth: usize,
    ) -> tsz_solver::types::TypeId {
        let Some(interner) = self.type_interner else {
            return type_id;
        };
        tsz_solver::visitor::widen_unique_symbol_value_type_for_dts(interner, type_id)
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

    fn rewrite_initializer_exported_import_equals_type_text(
        &self,
        initializer: NodeIndex,
        type_text: String,
    ) -> String {
        let type_text = self.rewrite_initializer_import_equals_type_text(initializer, type_text);
        self.rewrite_exported_import_equals_type_text(type_text)
    }

    pub(in crate::declaration_emitter) fn rewrite_initializer_import_equals_type_text(
        &self,
        initializer: NodeIndex,
        type_text: String,
    ) -> String {
        let Some((target, alias)) = self.initializer_import_equals_alias_rewrite(initializer)
        else {
            return type_text;
        };
        Self::replace_qualified_type_reference_prefix_text(&type_text, &target, &alias)
    }

    fn initializer_import_equals_alias_rewrite(
        &self,
        initializer: NodeIndex,
    ) -> Option<(String, String)> {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        let node = self.arena.get(initializer)?;
        match node.kind {
            k if k == syntax_kind_ext::NEW_EXPRESSION || k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
                self.expression_import_equals_alias_rewrite(call.expression)
            }
            _ => self.expression_import_equals_alias_rewrite(initializer),
        }
    }

    fn expression_import_equals_alias_rewrite(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let node = self.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.import_equals_alias_target_text_for_identifier(expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                self.expression_import_equals_alias_rewrite(access.expression)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn import_equals_alias_target_text_for_identifier(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let binder = self.binder?;
        let ident_node = self.arena.get(ident_idx)?;
        if ident_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let alias_name = self.get_identifier_text(ident_idx)?;
        let scope_id = binder.find_enclosing_scope(self.arena, ident_idx)?;
        let sym_id = self.resolve_name_in_scope_chain(binder, scope_id, &alias_name)?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::ALIAS == 0 {
            return None;
        }
        let import_idx = symbol.declarations.iter().copied().find(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        })?;
        let import_node = self.arena.get(import_idx)?;
        let import_decl = self.arena.get_import_decl(import_node)?;
        let target_text = self.entity_name_text(import_decl.module_specifier)?;
        if target_text == alias_name
            || self
                .arena
                .get(import_decl.module_specifier)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
        {
            return None;
        }
        Some((target_text, alias_name))
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

    fn replace_qualified_type_reference_prefix_text(
        type_text: &str,
        from: &str,
        to: &str,
    ) -> String {
        let mut out = String::with_capacity(type_text.len());
        let mut search_start = 0;

        while let Some(relative_idx) = type_text[search_start..].find(from) {
            let start = search_start + relative_idx;
            let end = start + from.len();
            out.push_str(&type_text[search_start..start]);
            let before = type_text[..start].chars().next_back();
            let after = type_text[end..].chars().next();
            let can_replace = !before.is_some_and(Self::is_qualified_type_reference_part)
                && (after == Some('.')
                    || !after.is_some_and(Self::is_qualified_type_reference_part));
            if can_replace {
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
}
