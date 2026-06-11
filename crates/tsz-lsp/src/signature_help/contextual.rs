use super::unicode_identifier::identifier_before_offset;
use super::*;
use tsz_checker::state::CheckerState;

impl<'a> SignatureHelpProvider<'a> {
    pub(super) fn signature_help_for_contextual_variable_initializer(
        &self,
        _root: NodeIndex,
        start_node: NodeIndex,
        cursor_offset: u32,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        let mut current = start_node;
        let mut declaration_idx = NodeIndex::NONE;
        if current.is_some() {
            while current.is_some() {
                let node = self.arena.get(current)?;
                if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                    declaration_idx = current;
                    break;
                }
                current = self.arena.get_extended(current)?.parent;
            }
        } else {
            // When a cursor sits in trivia (whitespace, comments) the AST lookup
            // can return no node. Recover by locating the tightest variable
            // declaration whose initializer span contains the cursor.
            let mut best_len = u32::MAX;
            for (idx, node) in self.arena.nodes.iter().enumerate() {
                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }
                let Some(decl) = self.arena.get_variable_declaration(node) else {
                    continue;
                };
                if !decl.initializer.is_some() {
                    continue;
                }
                let Some(init_node) = self.arena.get(decl.initializer) else {
                    continue;
                };
                if cursor_offset < init_node.pos || cursor_offset > init_node.end {
                    continue;
                }
                let len = node.end.saturating_sub(node.pos);
                if len < best_len {
                    best_len = len;
                    declaration_idx = NodeIndex(idx as u32);
                }
            }
        }
        if !declaration_idx.is_some() {
            return None;
        }

        let decl_node = self.arena.get(declaration_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        if !decl.type_annotation.is_some() || !decl.initializer.is_some() {
            return None;
        }
        let initializer_node = self.arena.get(decl.initializer)?;
        if cursor_offset < initializer_node.pos || cursor_offset > initializer_node.end {
            return None;
        }
        let lower_bound = initializer_node.pos;
        let open_paren = self.find_unmatched_open_paren_before(lower_bound, cursor_offset)?;
        let active_parameter =
            self.count_top_level_commas_in_range((open_paren + 1) as usize, cursor_offset as usize);
        let arg_count = self.textual_argument_count_for_open_paren(open_paren, cursor_offset);

        let var_name = self
            .arena
            .get_identifier_text(decl.name)
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        if var_name.is_empty() {
            return None;
        }
        let contextual_name = self
            .arena
            .get(decl.type_annotation)
            .and_then(|type_node| {
                if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                    return None;
                }
                let type_ref = self.arena.get_type_ref(type_node)?;
                self.arena
                    .get_identifier_text(type_ref.type_name)
                    .map(std::string::ToString::to_string)
            })
            .unwrap_or_else(|| var_name.clone());

        let mut checker = self.checker_with_cache(type_cache);

        let contextual_type = checker.get_type_of_node(decl.type_annotation);
        let contextual_type = checker.resolve_lazy_type(contextual_type);
        let mut signatures = self.get_signatures_from_type(
            contextual_type,
            &checker,
            CallKind::Call,
            &contextual_name,
            false,
            &[],
        );
        if signatures.is_empty()
            && initializer_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            let member_name = self
                .enclosing_object_member_name_within_argument(decl.initializer, cursor_offset)
                .map(|(name, _)| name)
                .or_else(|| {
                    self.object_member_name_from_argument_text(decl.initializer, cursor_offset)
                });
            if let Some(member_name) = member_name {
                if let Some(prop_type) =
                    self.contextual_property_type_from_type(contextual_type, &member_name)
                {
                    signatures = self.get_signatures_from_type(
                        prop_type,
                        &checker,
                        CallKind::Call,
                        &member_name,
                        false,
                        &[],
                    );
                } else if let Some(sig_info) =
                    self.source_contextual_member_signature(&checker, contextual_type, &member_name)
                {
                    signatures = vec![self.signature_candidate_from_info(sig_info)];
                }
            }
        }
        if signatures.is_empty()
            && let Some(type_node) = self.arena.get(decl.type_annotation)
        {
            let start = type_node.pos as usize;
            let end = (type_node.end as usize).min(self.source_text.len());
            if start < end {
                let type_text = self.source_text[start..end]
                    .trim()
                    .trim_end_matches('=')
                    .trim_end();
                if let Some(info) =
                    self.signature_info_from_member_signature_text(&contextual_name, type_text)
                {
                    signatures = vec![self.signature_candidate_from_info(info)];
                }
            }
        }
        for sig in &mut signatures {
            if !sig.type_param_substitutions.is_empty() {
                apply_type_param_substitution(&mut sig.info, &sig.type_param_substitutions);
            }
        }
        if signatures.is_empty() {
            *type_cache = Some(checker.extract_cache());
            return None;
        }
        *type_cache = Some(checker.extract_cache());

        let active_signature =
            self.select_active_signature(&signatures, arg_count, active_parameter, &[]);
        let active_parameter =
            self.clamp_active_parameter(&signatures, active_signature, active_parameter, arg_count);

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: arg_count as u32,
            applicable_span_start: open_paren + 1,
            applicable_span_length: cursor_offset.saturating_sub(open_paren + 1),
        })
    }

    pub(super) fn contextual_signature_help_from_call_argument(
        &self,
        call_expr: &CallExprData,
        cursor_offset: u32,
        callee_type: TypeId,
        checker: &CheckerState<'_>,
    ) -> Option<SignatureHelp> {
        let (arg_index, arg_node_idx) =
            self.argument_index_and_node_at_cursor(call_expr, cursor_offset)?;
        let (outer_param_type, outer_param_name) =
            self.parameter_type_and_name_at(callee_type, CallKind::Call, arg_index)?;
        let arg_node = self.arena.get(arg_node_idx)?;
        let mut source_signature: Option<SignatureInformation> = None;

        let (context_type, context_name, scan_start) = if let Some((member_name, member_idx)) =
            self.enclosing_object_member_name_within_argument(arg_node_idx, cursor_offset)
        {
            let member_node = self.arena.get(member_idx)?;
            if let Some(prop_type) =
                self.contextual_property_type_from_type(outer_param_type, &member_name)
            {
                (prop_type, member_name, member_node.pos)
            } else {
                let sig_info = self.source_contextual_member_signature(
                    checker,
                    outer_param_type,
                    &member_name,
                )?;
                source_signature = Some(sig_info);
                (TypeId::ERROR, member_name, member_node.pos)
            }
        } else if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && let Some(member_name) =
                self.object_member_name_from_argument_text(arg_node_idx, cursor_offset)
        {
            if let Some(prop_type) =
                self.contextual_property_type_from_type(outer_param_type, &member_name)
            {
                (prop_type, member_name, arg_node.pos)
            } else {
                let sig_info = self.source_contextual_member_signature(
                    checker,
                    outer_param_type,
                    &member_name,
                )?;
                source_signature = Some(sig_info);
                (TypeId::ERROR, member_name, arg_node.pos)
            }
        } else {
            let kind = arg_node.kind;
            let looks_like_callback = kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || kind == syntax_kind_ext::ARROW_FUNCTION;
            if !looks_like_callback {
                return None;
            }
            (
                outer_param_type,
                outer_param_name.unwrap_or_else(|| "callback".to_string()),
                arg_node.pos,
            )
        };

        let mut signatures = if let Some(sig_info) = source_signature {
            vec![self.signature_candidate_from_info(sig_info)]
        } else {
            self.get_signatures_from_type(
                context_type,
                checker,
                CallKind::Call,
                &context_name,
                false,
                &[],
            )
        };
        for sig in &mut signatures {
            if !sig.type_param_substitutions.is_empty() {
                apply_type_param_substitution(&mut sig.info, &sig.type_param_substitutions);
            }
        }
        if signatures.is_empty() {
            return None;
        }

        let open_paren = self.find_unmatched_open_paren_before(scan_start, cursor_offset)?;
        let contextual_active_parameter =
            self.count_top_level_commas_in_range((open_paren + 1) as usize, cursor_offset as usize);
        let arg_count = self.textual_argument_count_for_open_paren(open_paren, cursor_offset);
        let active_signature =
            self.select_active_signature(&signatures, arg_count, contextual_active_parameter, &[]);
        let active_parameter = self.clamp_active_parameter(
            &signatures,
            active_signature,
            contextual_active_parameter,
            arg_count,
        );

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: arg_count as u32,
            applicable_span_start: open_paren + 1,
            applicable_span_length: cursor_offset.saturating_sub(open_paren + 1),
        })
    }

    pub(super) fn argument_index_and_node_at_cursor(
        &self,
        call_expr: &CallExprData,
        cursor_offset: u32,
    ) -> Option<(usize, NodeIndex)> {
        let args = call_expr.arguments.as_ref()?;
        for (idx, &arg_idx) in args.nodes.iter().enumerate() {
            let node = self.arena.get(arg_idx)?;
            if node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            if cursor_offset > node.pos && cursor_offset <= node.end {
                return Some((idx, arg_idx));
            }
        }
        None
    }

    pub(super) fn object_member_name_from_argument_text(
        &self,
        argument_idx: NodeIndex,
        cursor_offset: u32,
    ) -> Option<String> {
        let arg_node = self.arena.get(argument_idx)?;
        let start = arg_node.pos as usize;
        let end = (cursor_offset as usize)
            .min(arg_node.end as usize)
            .min(self.source_text.len());
        if start >= end {
            return None;
        }
        let prefix = &self.source_text[start..end];
        let colon_candidate = prefix
            .rfind(':')
            .and_then(|idx| identifier_before_offset(prefix, idx).map(|name| (idx, name)));
        let paren_candidate = prefix.rfind('(').and_then(|idx| {
            let name = identifier_before_offset(prefix, idx)?;
            if name == "function" {
                return None;
            }
            Some((idx, name))
        });

        match (colon_candidate, paren_candidate) {
            (Some((ci, cname)), Some((pi, pname))) => {
                if pi > ci {
                    Some(pname)
                } else {
                    Some(cname)
                }
            }
            (Some((_, cname)), None) => Some(cname),
            (None, Some((_, pname))) => Some(pname),
            (None, None) => None,
        }
    }

    pub(super) fn parameter_type_and_name_at(
        &self,
        type_id: TypeId,
        call_kind: CallKind,
        arg_index: usize,
    ) -> Option<(TypeId, Option<String>)> {
        if let Some(shape_id) = visitor::function_shape_id(self.interner, type_id) {
            let shape = self.interner.function_shape(shape_id);
            return self.parameter_type_and_name_from_params(&shape.params, arg_index);
        }

        if let Some(shape_id) = visitor::callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(shape_id);
            let signatures = if call_kind == CallKind::New {
                &shape.construct_signatures
            } else {
                &shape.call_signatures
            };
            for sig in signatures {
                if let Some(found) =
                    self.parameter_type_and_name_from_params(&sig.params, arg_index)
                {
                    return Some(found);
                }
            }
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, type_id))
        {
            for &member in self.interner.type_list(list_id).iter() {
                if let Some(found) = self.parameter_type_and_name_at(member, call_kind, arg_index) {
                    return Some(found);
                }
            }
        }

        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            return self.parameter_type_and_name_at(app.base, call_kind, arg_index);
        }

        None
    }

    pub(super) fn parameter_type_and_name_from_params(
        &self,
        params: &[tsz_solver::ParamInfo],
        arg_index: usize,
    ) -> Option<(TypeId, Option<String>)> {
        if arg_index < params.len() {
            let param = params[arg_index];
            let name = param.name.map(|atom| self.interner.resolve_atom(atom));
            return Some((param.type_id, name));
        }
        params.last().and_then(|param| {
            if param.rest {
                let name = param.name.map(|atom| self.interner.resolve_atom(atom));
                Some((param.type_id, name))
            } else {
                None
            }
        })
    }

    pub(super) fn enclosing_object_member_name_within_argument(
        &self,
        argument_idx: NodeIndex,
        cursor_offset: u32,
    ) -> Option<(String, NodeIndex)> {
        let mut current =
            find_node_at_or_before_offset(self.arena, cursor_offset, self.source_text);
        while current.is_some() {
            if current == argument_idx {
                break;
            }
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let prop = self.arena.get_property_assignment(node)?;
                let name = self
                    .arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)?;
                return Some((name, current));
            }
            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let method = self.arena.get_method_decl(node)?;
                let name = self
                    .arena
                    .get_identifier_text(method.name)
                    .map(std::string::ToString::to_string)?;
                return Some((name, current));
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    pub(super) fn contextual_property_type_from_type(
        &self,
        container_type_id: TypeId,
        prop_name: &str,
    ) -> Option<TypeId> {
        if let Some(shape_id) = visitor::callable_shape_id(self.interner, container_type_id) {
            let shape = self.interner.callable_shape(shape_id);
            for prop in &shape.properties {
                if self.interner.resolve_atom(prop.name) == prop_name {
                    return Some(prop.type_id);
                }
            }
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, container_type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, container_type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            for prop in &shape.properties {
                if self.interner.resolve_atom(prop.name) == prop_name {
                    return Some(prop.type_id);
                }
            }
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, container_type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, container_type_id))
        {
            for &member in self.interner.type_list(list_id).iter() {
                if let Some(member_type) =
                    self.contextual_property_type_from_type(member, prop_name)
                {
                    return Some(member_type);
                }
            }
        }

        if let Some(app_id) = visitor::application_id(self.interner, container_type_id) {
            let app = self.interner.type_application(app_id);
            return self.contextual_property_type_from_type(app.base, prop_name);
        }

        None
    }

    pub(super) fn source_contextual_member_signature(
        &self,
        checker: &CheckerState<'_>,
        container_type_id: TypeId,
        member_name: &str,
    ) -> Option<SignatureInformation> {
        let container_type_text = checker.format_type(container_type_id);
        let interface_name = container_type_text
            .split('<')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        let signature_text =
            self.source_interface_member_signature_text(interface_name, member_name)?;
        self.signature_info_from_member_signature_text(member_name, &signature_text)
    }

    pub(super) fn source_interface_member_signature_text(
        &self,
        interface_name: &str,
        member_name: &str,
    ) -> Option<String> {
        let iface_pattern = format!("interface {interface_name}");
        let iface_start = self.source_text.find(&iface_pattern)?;
        let after_iface = &self.source_text[iface_start..];
        let body_open_rel = after_iface.find('{')?;
        let body_open = iface_start + body_open_rel;
        let body_close = self.find_matching_brace_in_source(body_open)?;
        let body = &self.source_text[body_open + 1..body_close];

        let method_pattern = format!("{member_name}(");
        if let Some(method_idx) = body.find(&method_pattern) {
            let tail = &body[method_idx..];
            let end = tail.find(';').unwrap_or(tail.len());
            return Some(tail[..end].trim().to_string());
        }

        let property_pattern = format!("{member_name}:");
        if let Some(property_idx) = body.find(&property_pattern) {
            let tail = &body[property_idx + property_pattern.len()..];
            let end = tail.find(';').unwrap_or(tail.len());
            return Some(tail[..end].trim().to_string());
        }

        None
    }

    pub(super) fn signature_info_from_member_signature_text(
        &self,
        member_name: &str,
        signature_text: &str,
    ) -> Option<SignatureInformation> {
        let trimmed = signature_text.trim().trim_end_matches(';').trim();
        let (params_text, return_type) = if trimmed.starts_with(member_name) {
            let open = trimmed.find('(')?;
            let close = Self::find_matching_paren_in_text(trimmed, open)?;
            let params = trimmed[open + 1..close].trim();
            let after = trimmed[close + 1..].trim();
            let return_type = after.strip_prefix(':')?.trim();
            (params, return_type)
        } else {
            let open = trimmed.find('(')?;
            let close = Self::find_matching_paren_in_text(trimmed, open)?;
            let params = trimmed[open + 1..close].trim();
            let after = trimmed[close + 1..].trim();
            let return_type = after.strip_prefix("=>")?.trim();
            (params, return_type)
        };

        let param_parts = Self::split_top_level_text(params_text, ',');
        let mut parameters = Vec::with_capacity(param_parts.len());
        for (idx, raw) in param_parts.into_iter().enumerate() {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let is_rest = raw.starts_with("...");
            let lhs = if let Some(colon_idx) = Self::find_top_level_char(raw, ':') {
                raw[..colon_idx].trim()
            } else {
                raw
            };
            let lhs = lhs.trim_start_matches("...").trim();
            let is_optional = !is_rest && lhs.ends_with('?');
            let mut name = lhs.trim_end_matches('?').trim().to_string();
            if name.is_empty() {
                name = format!("arg{idx}");
            }
            parameters.push(ParameterInformation {
                name,
                label: raw.to_string(),
                documentation: None,
                is_optional,
                is_rest,
            });
        }
        let labels: Vec<String> = parameters.iter().map(|p| p.label.clone()).collect();
        let is_variadic = parameters.iter().any(|p| p.is_rest);
        let prefix = format!("{member_name}(");
        let suffix = format!("): {return_type}");
        Some(SignatureInformation {
            label: format!("{prefix}{}{}", labels.join(", "), suffix),
            prefix,
            suffix,
            documentation: None,
            parameters,
            is_variadic,
            is_constructor: false,
            tags: Vec::new(),
        })
    }

    pub(super) fn signature_candidate_from_info(
        &self,
        info: SignatureInformation,
    ) -> SignatureCandidate {
        let required_params = info
            .parameters
            .iter()
            .filter(|param| !param.is_optional && !param.is_rest)
            .count();
        let total_params = info.parameters.len();
        let has_rest = info.parameters.iter().any(|param| param.is_rest);
        let param_names = info
            .parameters
            .iter()
            .map(|param| Some(param.name.clone()))
            .collect();
        SignatureCandidate {
            info,
            required_params,
            total_params,
            has_rest,
            param_names,
            type_params: Vec::new(),
            type_param_substitutions: Vec::new(),
        }
    }

    pub(super) fn find_matching_brace_in_source(&self, open_brace: usize) -> Option<usize> {
        let bytes = self.source_text.as_bytes();
        if open_brace >= bytes.len() || bytes[open_brace] != b'{' {
            return None;
        }
        let mut depth = 0i32;
        for (idx, byte) in bytes.iter().enumerate().skip(open_brace) {
            match *byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(super) fn find_matching_paren_in_text(text: &str, open_paren: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        if open_paren >= bytes.len() || bytes[open_paren] != b'(' {
            return None;
        }
        let mut depth = 0i32;
        for (idx, byte) in bytes.iter().enumerate().skip(open_paren) {
            match *byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(super) fn find_unmatched_open_paren_before(
        &self,
        lower_bound: u32,
        cursor_offset: u32,
    ) -> Option<u32> {
        let bytes = self.source_text.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let cursor = cursor_offset.min(bytes.len() as u32);
        if (cursor as usize) < bytes.len() && bytes[cursor as usize] == b'(' {
            return Some(cursor);
        }
        let mut depth = 0i32;
        let min = lower_bound.min(bytes.len() as u32) as i64;
        let mut idx = (cursor as i64).saturating_sub(1);
        while idx >= min && idx >= 0 {
            match bytes[idx as usize] {
                b')' => depth += 1,
                b'(' => {
                    if depth == 0 {
                        return Some(idx as u32);
                    }
                    depth -= 1;
                }
                _ => {}
            }
            idx -= 1;
        }
        None
    }

    pub(super) fn textual_argument_count_for_open_paren(
        &self,
        open_paren: u32,
        cursor_offset: u32,
    ) -> usize {
        let start = (open_paren + 1).min(self.source_text.len() as u32) as usize;
        let end = cursor_offset.min(self.source_text.len() as u32) as usize;
        let text = self.source_text.get(start..end).unwrap_or_default();
        let active = self.count_top_level_commas_in_range(start, end) as usize;
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            0
        } else if trimmed.ends_with(',') {
            active
        } else {
            active + 1
        }
    }

    pub(super) fn clamp_active_parameter(
        &self,
        signatures: &[SignatureCandidate],
        active_signature: u32,
        active_parameter: u32,
        arg_count: usize,
    ) -> u32 {
        let mut out = active_parameter;
        if let Some(selected) = signatures.get(active_signature as usize) {
            if selected.info.parameters.is_empty() {
                out = 0;
            } else {
                let has_rest_param = selected.info.parameters.iter().any(|param| param.is_rest);
                let max_index = selected.info.parameters.len().saturating_sub(1);
                if has_rest_param {
                    if out as usize >= arg_count && out as usize > max_index {
                        out = max_index as u32;
                    }
                } else if out as usize > max_index {
                    out = max_index as u32;
                }
            }
        }
        out
    }

    /// For a `super` keyword node, walk up to find the enclosing class, then
    /// return the expression from its `extends` clause (the base class reference).
    /// This lets us resolve the base class symbol for signature help on `super()`.
    pub(super) fn find_base_class_expression(&self, super_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = super_idx;
        let mut depth = 0;
        while current.is_some() && depth < 100 {
            let node = self.arena.get(current)?;
            if node.is_class_like() {
                let class_data = self.arena.get_class(node)?;
                let heritage_clauses = class_data.heritage_clauses.as_ref()?;
                for &clause_idx in &heritage_clauses.nodes {
                    let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    let &type_idx = heritage.types.nodes.first()?;
                    // The type in the heritage clause is an ExpressionWithTypeArguments node.
                    // We need the expression inside it (the base class identifier).
                    if let Some(expr_type_args) = self.arena.get_expr_type_args_at(type_idx) {
                        return Some(expr_type_args.expression);
                    }
                    // If not wrapped in ExpressionWithTypeArguments, use directly
                    return Some(type_idx);
                }
                return None;
            }
            if let Some(extended) = self.arena.get_extended(current) {
                current = extended.parent;
            } else {
                break;
            }
            depth += 1;
        }
        None
    }

    /// Determine active parameter by scanning for commas, respecting nesting.
    /// This is more robust than AST analysis for incomplete code.
    pub(super) fn determine_active_parameter(
        &self,
        call_idx: NodeIndex,
        data: &CallExprData,
        cursor_offset: u32,
    ) -> u32 {
        // Use AST-based approach instead of token scanning to handle edge cases:
        // - Generic type arguments with angle brackets: Set<string, number>
        // - Nested calls: foo(bar(x, y), z)
        // - Complex expressions with comparison operators: a < b

        // If there are no arguments, return 0
        let Some(ref args) = data.arguments else {
            return 0;
        };

        // Check if cursor is before the first argument
        if args.nodes.is_empty() {
            return 0;
        }

        let mut seen_non_omitted = 0usize;
        let mut last_non_omitted_end = None;

        for (index, &arg_idx) in args.nodes.iter().enumerate() {
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }

            // If cursor is before this argument's start, we're between args
            // Treat it as the next argument.
            if cursor_offset <= arg_node.pos {
                return seen_non_omitted as u32;
            }

            // If cursor is within this argument's range, return this index
            if cursor_offset <= arg_node.end {
                return seen_non_omitted as u32;
            }

            let next_start = args
                .nodes
                .iter()
                .skip(index + 1)
                .filter_map(|&next_idx| self.arena.get(next_idx))
                .find(|next| next.kind != syntax_kind_ext::OMITTED_EXPRESSION)
                .map(|next| next.pos);
            if let Some(next_start) = next_start
                && cursor_offset < next_start
            {
                return (seen_non_omitted + 1) as u32;
            }
            last_non_omitted_end = Some(arg_node.end as usize);
            seen_non_omitted += 1;
        }

        if let Some(last_end) = last_non_omitted_end {
            let end = (cursor_offset as usize).min(self.source_text.len());
            if last_end < end
                && seen_non_omitted > 0
                && !self.has_comma_between(last_end as u32, cursor_offset)
            {
                return (seen_non_omitted - 1) as u32;
            }
        }

        if let Some(call_node) = self.arena.get(call_idx)
            && let Some(last_end) = last_non_omitted_end
        {
            let scan_end = cursor_offset.min(call_node.end);
            if self.has_comma_between(last_end as u32, scan_end) {
                return seen_non_omitted as u32;
            }
        }

        seen_non_omitted as u32
    }

    pub(super) fn has_comma_between(&self, start: u32, end: u32) -> bool {
        has_comma_between_offsets(self.source_text, start as usize, end as usize)
    }

    pub(super) fn count_top_level_commas_in_range(&self, start: usize, end: usize) -> u32 {
        count_top_level_commas(self.source_text, start, end)
    }

    pub(super) fn type_argument_context_for_call(
        &self,
        call_idx: NodeIndex,
        data: &CallExprData,
        cursor_offset: u32,
    ) -> Option<TypeArgumentContext> {
        data.type_arguments.as_ref()?;

        let call_node = self.arena.get(call_idx)?;
        let call_start = call_node.pos as usize;
        let call_end = (call_node.end as usize).min(self.source_text.len());
        if call_start >= call_end {
            return None;
        }
        let call_text = &self.source_text[call_start..call_end];
        let lt_rel = call_text.find('<')?;
        let lt_abs = call_start + lt_rel;
        if cursor_offset <= lt_abs as u32 {
            return None;
        }

        let bytes = self.source_text.as_bytes();
        let mut depth = 0i32;
        let mut gt_abs = None;
        let mut i = lt_abs;
        while i < call_end {
            match bytes[i] {
                b'<' => depth += 1,
                b'>' if i == 0 || bytes[i - 1] != b'=' => {
                    depth -= 1;
                    if depth == 0 {
                        gt_abs = Some(i);
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        if let Some(gt) = gt_abs
            && cursor_offset > gt as u32
        {
            return None;
        }

        let scan_end = gt_abs.map_or(cursor_offset as usize, |gt| {
            (cursor_offset as usize).min(gt)
        });
        let scan_start = (lt_abs + 1).min(scan_end);
        let active_parameter = self.count_top_level_commas_in_range(scan_start, scan_end);

        Some(TypeArgumentContext {
            active_parameter,
            span_start: scan_start as u32,
            span_length: (scan_end.saturating_sub(scan_start)) as u32,
        })
    }

    pub(super) fn rewrite_signatures_for_type_arguments(
        &self,
        signatures: &mut Vec<SignatureCandidate>,
        callee_name: &str,
        active_parameter: u32,
    ) {
        let has_generic_signatures = signatures
            .iter()
            .any(|candidate| !candidate.type_params.is_empty());
        if has_generic_signatures {
            signatures.retain(|candidate| {
                !candidate.type_params.is_empty()
                    && (active_parameter as usize) < candidate.type_params.len()
            });
        }
        for candidate in signatures.iter_mut() {
            let params = if has_generic_signatures {
                candidate.type_params.clone()
            } else {
                Vec::new()
            };
            let function_tail = candidate
                .info
                .label
                .find('(')
                .map(|idx| candidate.info.label[idx..].to_string())
                .unwrap_or_else(|| "()".to_string());
            let prefix = format!("{callee_name}<");
            let suffix = format!(">{function_tail}");
            candidate.info.parameters = params
                .iter()
                .map(|param| {
                    let name = param
                        .split_once(" extends ")
                        .map_or_else(|| param.clone(), |(name, _)| name.to_string());
                    ParameterInformation {
                        name,
                        label: param.clone(),
                        documentation: None,
                        is_optional: false,
                        is_rest: false,
                    }
                })
                .collect();
            candidate.info.prefix = prefix.clone();
            candidate.info.suffix = suffix.clone();
            candidate.info.label = format!("{prefix}{}{}", params.join(", "), suffix);
            candidate.info.is_variadic = false;
            candidate.required_params = params.len();
            candidate.total_params = params.len();
            candidate.has_rest = false;
            candidate.param_names = params
                .iter()
                .map(|param| {
                    Some(
                        param
                            .split_once(" extends ")
                            .map_or_else(|| param.clone(), |(name, _)| name.to_string()),
                    )
                })
                .collect();
        }
    }

    pub(super) fn find_textual_call_trigger(
        &self,
        cursor_offset: u32,
    ) -> Option<TextualTypeArgumentTrigger> {
        let ctx = find_incomplete_paren_call(self.source_text, cursor_offset as usize)?;
        Some(TextualTypeArgumentTrigger {
            callee_name: ctx.callee_name,
            callee_offset: ctx.callee_end_offset.saturating_sub(1) as u32,
            call_kind: if ctx.is_new_expression {
                CallKind::New
            } else {
                CallKind::Call
            },
            active_parameter: ctx.active_parameter,
            span_start: ctx.span_start as u32,
            span_length: ctx.span_length as u32,
        })
    }

    pub(super) fn find_textual_type_argument_trigger(
        &self,
        cursor_offset: u32,
    ) -> Option<TextualTypeArgumentTrigger> {
        // Robustness audit (PR #F, item 6 in
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`): emit a
        // structured trace at every invocation so the rate at which
        // signature help depends on source-text scanning is visible.
        tracing::trace!(
            site = "signature_help::find_textual_type_argument_trigger",
            cursor_offset = cursor_offset,
            "LSP signature help fell back to text-scanning for type-argument trigger"
        );
        let ctx = find_incomplete_angle_call(self.source_text, cursor_offset as usize)?;
        Some(TextualTypeArgumentTrigger {
            callee_name: ctx.callee_name,
            callee_offset: ctx.callee_end_offset.saturating_sub(1) as u32,
            call_kind: if ctx.is_new_expression {
                CallKind::New
            } else {
                CallKind::Call
            },
            active_parameter: ctx.active_parameter,
            span_start: ctx.span_start as u32,
            span_length: ctx.span_length as u32,
        })
    }

    pub(super) fn signature_help_for_textual_call(
        &self,
        root: NodeIndex,
        cursor_offset: u32,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        // Audit PR #F: see `find_textual_type_argument_trigger`.
        tracing::trace!(
            site = "signature_help::signature_help_for_textual_call",
            cursor_offset = cursor_offset,
            "LSP signature help fell back to text-scanning for incomplete call site"
        );
        let trigger = self.find_textual_call_trigger(cursor_offset)?;
        let callee_expr =
            self.find_identifier_node_at_offset(trigger.callee_offset, &trigger.callee_name)?;

        let mut walker = crate::resolver::ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, callee_expr)?;

        let mut checker = self.checker_with_cache(type_cache);

        let docs = self.signature_documentation_for_symbol(root, symbol_id, trigger.call_kind);
        let callee_type = checker.get_type_of_symbol(symbol_id);
        let callee_type = checker.resolve_lazy_type(callee_type);
        let mut signatures = self.get_signatures_from_type(
            callee_type,
            &checker,
            trigger.call_kind,
            &trigger.callee_name,
            false,
            &[],
        );

        if let Some(docs) = docs {
            self.apply_signature_docs(&mut signatures, &docs);
        }
        self.apply_source_signature_type_overrides(&mut signatures, symbol_id);
        for sig in &mut signatures {
            if !sig.type_param_substitutions.is_empty() {
                apply_type_param_substitution(&mut sig.info, &sig.type_param_substitutions);
            }
        }
        self.expand_source_rest_tuple_union_signatures(&mut signatures, symbol_id);
        if signatures.is_empty() {
            *type_cache = Some(checker.extract_cache());
            return None;
        }

        *type_cache = Some(checker.extract_cache());

        let span_start = trigger.span_start as usize;
        let span_end = span_start + trigger.span_length as usize;
        let span_text = self
            .source_text
            .get(span_start..span_end)
            .unwrap_or_default();
        let trimmed = span_text.trim_end();
        let arg_count = if trimmed.is_empty() {
            0usize
        } else if trimmed.ends_with(',') {
            trigger.active_parameter as usize
        } else {
            trigger.active_parameter as usize + 1
        };

        let active_signature =
            self.select_active_signature(&signatures, arg_count, trigger.active_parameter, &[]);
        let mut active_parameter = trigger.active_parameter;
        if let Some(selected) = signatures.get(active_signature as usize) {
            if selected.info.parameters.is_empty() {
                active_parameter = 0;
            } else {
                let has_rest_param = selected.info.parameters.iter().any(|param| param.is_rest);
                let max_index = selected.info.parameters.len().saturating_sub(1);
                if has_rest_param {
                    if active_parameter as usize >= arg_count
                        && active_parameter as usize > max_index
                    {
                        active_parameter = max_index as u32;
                    }
                } else if active_parameter as usize > max_index {
                    active_parameter = max_index as u32;
                }
            }
        }

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: arg_count as u32,
            applicable_span_start: trigger.span_start,
            applicable_span_length: trigger.span_length,
        })
    }

    pub(super) fn find_identifier_node_at_offset(
        &self,
        offset: u32,
        expected_name: &str,
    ) -> Option<NodeIndex> {
        let mut current = find_node_at_or_before_offset(self.arena, offset, self.source_text);
        let mut depth = 0usize;
        while current.is_some() && depth < 128 {
            let node = self.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16
                && self.arena.get_identifier_text(current) == Some(expected_name)
            {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
            depth += 1;
        }
        None
    }

    pub(super) fn signature_help_for_textual_type_arguments(
        &self,
        root: NodeIndex,
        cursor_offset: u32,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        // Audit PR #F: see `find_textual_type_argument_trigger`.
        tracing::trace!(
            site = "signature_help::signature_help_for_textual_type_arguments",
            cursor_offset = cursor_offset,
            "LSP signature help fell back to text-scanning for type-argument completion"
        );
        let trigger = self.find_textual_type_argument_trigger(cursor_offset)?;
        let callee_expr =
            self.find_identifier_node_at_offset(trigger.callee_offset, &trigger.callee_name)?;

        let mut walker = crate::resolver::ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, callee_expr)?;

        let mut checker = self.checker_with_cache(type_cache);

        let docs = self.signature_documentation_for_symbol(root, symbol_id, trigger.call_kind);
        let callee_type = checker.get_type_of_symbol(symbol_id);
        let callee_type = checker.resolve_lazy_type(callee_type);
        let mut signatures = self.get_signatures_from_type(
            callee_type,
            &checker,
            trigger.call_kind,
            &trigger.callee_name,
            false,
            &[],
        );

        if let Some(docs) = docs {
            self.apply_signature_docs(&mut signatures, &docs);
        }
        self.apply_source_signature_type_overrides(&mut signatures, symbol_id);
        self.rewrite_signatures_for_type_arguments(
            &mut signatures,
            &trigger.callee_name,
            trigger.active_parameter,
        );
        if signatures.is_empty() {
            *type_cache = Some(checker.extract_cache());
            return None;
        }

        *type_cache = Some(checker.extract_cache());

        let active_signature =
            self.select_active_signature(&signatures, 0, trigger.active_parameter, &[]);
        let mut active_parameter = trigger.active_parameter;
        if let Some(selected) = signatures.get(active_signature as usize) {
            if selected.info.parameters.is_empty() {
                active_parameter = 0;
            } else {
                let has_rest_param = selected.info.parameters.iter().any(|param| param.is_rest);
                if !has_rest_param {
                    let max_index = selected.info.parameters.len().saturating_sub(1);
                    if active_parameter as usize > max_index {
                        active_parameter = max_index as u32;
                    }
                }
            }
        }

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: 0,
            applicable_span_start: trigger.span_start,
            applicable_span_length: trigger.span_length,
        })
    }
}
