//! Type checking query helpers: type parameter scope, function implementation
//! checking, and class member analysis.
//!
//! Library type resolution (`resolve_lib_type_by_name`, `merge_lib_interface_heritage`)
//! has been extracted to `queries/lib_resolution.rs`.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Section 39: Type Parameter Scope Utilities
    // =========================================================================

    /// Pop type parameters from scope, restoring previous values.
    /// Used to restore the type parameter scope after exiting a generic context.
    pub(crate) fn pop_type_parameters(&mut self, updates: Vec<(String, Option<TypeId>, bool)>) {
        for (name, previous, shadowed_class_param) in updates.into_iter().rev() {
            if let Some(prev_type) = previous {
                self.ctx
                    .type_parameter_scope
                    .insert(name.clone(), prev_type);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
            if shadowed_class_param && let Some(ref mut c) = self.ctx.enclosing_class {
                c.type_param_names.push(name);
            }
        }
    }

    /// Push parameter names into `typeof_param_scope` so that `typeof paramName`
    /// in return type annotations can resolve to the parameter's declared type.
    pub(crate) fn push_typeof_param_scope(&mut self, params: &[tsz_solver::ParamInfo]) {
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.insert(name, param.type_id);
            }
        }
    }

    /// Remove parameter names from `typeof_param_scope` after return type resolution.
    pub(crate) fn pop_typeof_param_scope(&mut self, params: &[tsz_solver::ParamInfo]) {
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.remove(&name);
            }
        }
    }

    /// Check for unused type parameters in a declaration and emit TS6133.
    ///
    /// This scans all identifiers within the declaration body for type parameter
    /// name references. Any type parameter that is not referenced gets a TS6133
    /// diagnostic. Called only from the checking path (not type resolution).
    pub(crate) fn check_unused_type_params(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
        body_root: NodeIndex,
    ) {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Type parameters are checked under noUnusedParameters, not noUnusedLocals.
        // See: unusedTypeParametersNotCheckedByNoUnusedLocals conformance test.
        if !self.ctx.no_unused_parameters() {
            return;
        }

        let Some(list) = type_parameters else {
            return;
        };

        // Collect type parameter names and their declaration name NodeIndices
        let mut params: Vec<(String, NodeIndex, bool)> = Vec::new();
        for (param_pos, &param_idx) in list.nodes.iter().enumerate() {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };
            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|id_data| id_data.escaped_text.clone())
                .unwrap_or_default();
            if !name.is_empty() && !name.starts_with('_') {
                params.push((name, data.name, list.nodes.len() == 1 && param_pos == 0));
            }
        }

        if params.is_empty() {
            return;
        }

        let Some(root_node) = self.ctx.arena.get(body_root) else {
            return;
        };
        let mut pos_start = root_node.pos;
        let mut pos_end = root_node.end;

        // Determine if this declaration is part of a cross-file merge.
        // For merged declarations (e.g., class C<T> in a.ts + interface C<T> in b.ts),
        // TSC checks type parameter usage across ALL merged declarations. If T is used
        // in ANY merged declaration, no TS6133 is emitted for any of them. If T is
        // unused in ALL merged declarations, TS6133 is emitted only for non-class
        // declarations (interfaces get flagged, classes do not).
        let mut is_cross_file_merge = false;
        let mut is_class_in_merge = false;
        let mut remote_decl_indices: Vec<(
            std::sync::Arc<tsz_parser::parser::NodeArena>,
            NodeIndex,
        )> = Vec::new();

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(body_root)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            // Check each declaration for local vs remote and expand local range.
            // Note: symbol.declarations may have fewer entries than actual merged
            // declarations when cross-file NodeIndex collision caused dedup in
            // parallel.rs. A single NodeIndex can map to multiple arenas in
            // declaration_arenas, so we must check arenas regardless of
            // symbol.declarations.len().
            for &decl_idx in &symbol.declarations {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    let mut has_local = false;
                    for arena_arc in arenas {
                        if std::ptr::eq(&**arena_arc, self.ctx.arena) {
                            has_local = true;
                        } else {
                            is_cross_file_merge = true;
                            remote_decl_indices.push((std::sync::Arc::clone(arena_arc), decl_idx));
                        }
                    }
                    if has_local && let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                        pos_start = pos_start.min(decl_node.pos);
                        pos_end = pos_end.max(decl_node.end);
                    }
                } else {
                    // No declaration_arenas entry: assume local
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                        pos_start = pos_start.min(decl_node.pos);
                        pos_end = pos_end.max(decl_node.end);
                    }
                }
            }

            // If this is a class declaration in a cross-file merge, TSC does not
            // emit TS6133 for the class's type parameters (only for interfaces).
            if is_cross_file_merge {
                let body_kind = root_node.kind;
                if body_kind == syntax_kind_ext::CLASS_DECLARATION
                    || body_kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    is_class_in_merge = true;
                }
            }
        }

        let decl_indices: Vec<NodeIndex> = params.iter().map(|(_, idx, _)| *idx).collect();
        let mut used = vec![false; params.len()];
        let is_identifier_in_type_context =
            |arena: &tsz_parser::parser::NodeArena, idx: NodeIndex, stop_at: NodeIndex| {
                let mut current = idx;
                for _ in 0..20 {
                    let Some(ext) = arena.get_extended(current) else {
                        return false;
                    };
                    let parent = ext.parent;
                    if parent.is_none() || parent == stop_at {
                        return false;
                    }
                    let Some(parent_node) = arena.get(parent) else {
                        return false;
                    };
                    if parent_node.is_type_node()
                        || parent_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                    {
                        return true;
                    }
                    current = parent;
                }
                false
            };

        // Scan all nodes in the LOCAL arena for identifiers within the declaration range
        let arena_len = self.ctx.arena.len();
        for i in 0..arena_len {
            let idx = NodeIndex(i as u32);
            // Skip the type parameter declaration identifiers themselves
            if decl_indices.contains(&idx) {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.pos < pos_start || node.end > pos_end {
                continue;
            }
            if node.kind == SyntaxKind::Identifier as u16
                && is_identifier_in_type_context(self.ctx.arena, idx, body_root)
                && let Some(ident) = self.ctx.arena.get_identifier(node)
            {
                let name_str = ident.escaped_text.as_str();
                for (j, (param_name, _, _)) in params.iter().enumerate() {
                    if !used[j] && param_name == name_str {
                        used[j] = true;
                    }
                }
            }
        }

        // For cross-file merges, also scan REMOTE arenas for type parameter usage.
        // This matches TSC behavior where T is considered "used" if it appears in
        // ANY merged declaration across files.
        if is_cross_file_merge && used.iter().any(|u| !u) {
            for (remote_arena, remote_decl_idx) in &remote_decl_indices {
                if let Some(remote_decl_node) = remote_arena.get(*remote_decl_idx) {
                    let remote_start = remote_decl_node.pos;
                    let remote_end = remote_decl_node.end;

                    // Collect type parameter declaration name identifiers in the
                    // remote arena so we can skip them (they are declarations, not
                    // usages of the type parameter).
                    let mut remote_tp_name_indices: Vec<NodeIndex> = Vec::new();
                    let remote_len = remote_arena.len();
                    for i in 0..remote_len {
                        let idx = NodeIndex(i as u32);
                        let Some(node) = remote_arena.get(idx) else {
                            continue;
                        };
                        if node.pos < remote_start || node.end > remote_end {
                            continue;
                        }
                        if let Some(tp_data) = remote_arena.get_type_parameter(node) {
                            remote_tp_name_indices.push(tp_data.name);
                        }
                    }

                    for i in 0..remote_len {
                        let idx = NodeIndex(i as u32);
                        // Skip type parameter declaration identifiers
                        if remote_tp_name_indices.contains(&idx) {
                            continue;
                        }
                        let Some(node) = remote_arena.get(idx) else {
                            continue;
                        };
                        if node.pos < remote_start || node.end > remote_end {
                            continue;
                        }
                        if node.kind == SyntaxKind::Identifier as u16
                            && is_identifier_in_type_context(remote_arena, idx, *remote_decl_idx)
                            && let Some(ident) = remote_arena.get_identifier(node)
                        {
                            let name_str = ident.escaped_text.as_str();
                            for (j, (param_name, _, _)) in params.iter().enumerate() {
                                if !used[j] && param_name == name_str {
                                    used[j] = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Emit TS6133 for unused type parameters.
        // For class declarations in a cross-file merge, TSC does not emit TS6133
        // (only interfaces in the merge get flagged).
        if is_class_in_merge {
            return;
        }

        for (j, (name, decl_idx, use_list_anchor)) in params.iter().enumerate() {
            if used[j] {
                continue;
            }
            if let Some(name_node) = self.ctx.arena.get(*decl_idx) {
                let start = if *use_list_anchor {
                    // Match tsc: single type-parameter lists anchor at '<'.
                    name_node.pos.saturating_sub(1)
                } else {
                    name_node.pos
                };
                let length = name_node.end.saturating_sub(name_node.pos);
                self.error_declared_but_never_read(name, start, length);
            }
        }
    }

    /// Check JSDoc `@template` type parameters for JS declarations that do not
    /// have syntax-level `<T>` lists.
    pub(crate) fn check_unused_jsdoc_template_type_params(&mut self, decl_idx: NodeIndex) {
        use tsz_scanner::SyntaxKind;

        if !self.ctx.no_unused_parameters() || !self.is_js_file() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some((_jsdoc, comment_pos)) =
            self.try_leading_jsdoc_with_pos(comments, node.pos, source_text)
        else {
            return;
        };

        let comment_end = node.pos.min(source_text.len() as u32) as usize;
        let raw_comment = &source_text[comment_pos as usize..comment_end];
        let params = Self::jsdoc_template_param_declarations(raw_comment, comment_pos);
        if params.is_empty() {
            return;
        }

        let mut used = vec![false; params.len()];
        let is_identifier_in_type_context =
            |arena: &tsz_parser::parser::NodeArena, idx: NodeIndex, stop_at: NodeIndex| {
                let mut current = idx;
                for _ in 0..20 {
                    let Some(ext) = arena.get_extended(current) else {
                        return false;
                    };
                    let parent = ext.parent;
                    if parent.is_none() || parent == stop_at {
                        return false;
                    }
                    let Some(parent_node) = arena.get(parent) else {
                        return false;
                    };
                    if parent_node.is_type_node()
                        || parent_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                    {
                        return true;
                    }
                    current = parent;
                }
                false
            };

        let arena_len = self.ctx.arena.len();
        for i in 0..arena_len {
            let idx = NodeIndex(i as u32);
            let Some(candidate) = self.ctx.arena.get(idx) else {
                continue;
            };
            if candidate.pos < node.pos || candidate.end > node.end {
                continue;
            }
            if candidate.kind == SyntaxKind::Identifier as u16
                && is_identifier_in_type_context(self.ctx.arena, idx, decl_idx)
                && let Some(ident) = self.ctx.arena.get_identifier(candidate)
            {
                let name_str = ident.escaped_text.as_str();
                for (j, (param_name, _, _)) in params.iter().enumerate() {
                    if !used[j] && param_name == name_str {
                        used[j] = true;
                    }
                }
            }
        }

        for type_expr in Self::jsdoc_type_expressions(raw_comment) {
            for (j, (param_name, _, _)) in params.iter().enumerate() {
                if !used[j] && Self::jsdoc_type_expr_mentions_name(type_expr, param_name) {
                    used[j] = true;
                }
            }
        }

        for (j, (name, start, length)) in params.iter().enumerate() {
            if !used[j] {
                self.error_declared_but_never_read(name, *start, *length);
            }
        }
    }

    fn jsdoc_template_param_declarations(
        raw_comment: &str,
        comment_pos: u32,
    ) -> Vec<(String, u32, u32)> {
        let mut params = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel) = raw_comment[cursor..].find("@template") {
            let tag_start = cursor + rel;
            let mut idx = cursor + rel + "@template".len();
            while let Some(ch) = raw_comment[idx..].chars().next() {
                if ch == ' ' || ch == '\t' || ch == '*' {
                    idx += ch.len_utf8();
                } else {
                    break;
                }
            }

            while let Some(ch) = raw_comment[idx..].chars().next() {
                if ch == '\n' || ch == '\r' || ch == '@' || ch == '{' {
                    break;
                }
                if ch == ',' || ch == ' ' || ch == '\t' || ch == '*' {
                    idx += ch.len_utf8();
                    continue;
                }
                if ch == '_' || ch == '$' || ch.is_ascii_alphabetic() {
                    let start = idx;
                    idx += ch.len_utf8();
                    while let Some(next) = raw_comment[idx..].chars().next() {
                        if next == '_' || next == '$' || next.is_ascii_alphanumeric() {
                            idx += next.len_utf8();
                        } else {
                            break;
                        }
                    }
                    let name = &raw_comment[start..idx];
                    if !name.starts_with('_') {
                        params.push((
                            name.to_string(),
                            comment_pos + tag_start as u32,
                            idx.saturating_sub(tag_start) as u32,
                        ));
                    }
                    continue;
                }
                break;
            }

            cursor = idx;
        }
        params
    }

    fn jsdoc_type_expressions(raw_comment: &str) -> Vec<&str> {
        let mut exprs = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel) = raw_comment[cursor..].find('{') {
            let start = cursor + rel + 1;
            let Some(end_rel) = raw_comment[start..].find('}') else {
                break;
            };
            exprs.push(&raw_comment[start..start + end_rel]);
            cursor = start + end_rel + 1;
        }
        exprs
    }

    fn jsdoc_type_expr_mentions_name(type_expr: &str, name: &str) -> bool {
        fn is_ident_char(ch: char) -> bool {
            ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
        }

        let mut cursor = 0usize;
        while let Some(rel) = type_expr[cursor..].find(name) {
            let start = cursor + rel;
            let end = start + name.len();
            let prev_ok = type_expr[..start]
                .chars()
                .next_back()
                .is_none_or(|ch| !is_ident_char(ch));
            let next_ok = type_expr[end..]
                .chars()
                .next()
                .is_none_or(|ch| !is_ident_char(ch));
            if prev_ok && next_ok {
                return true;
            }
            cursor = end;
        }
        false
    }

    /// Collect all `infer` type parameter names from a type node.
    /// This is used to add inferred type parameters to the scope when checking conditional types.
    pub(crate) fn collect_infer_type_parameters(&self, type_idx: NodeIndex) -> Vec<String> {
        let mut params = Vec::new();
        self.collect_infer_type_parameters_inner(type_idx, &mut params);
        params
    }

    /// Collect all `infer` type parameter declarations with their constraint and position info.
    /// Returns `(name, constraint_node_idx, type_parameter_node_idx)` for each `infer` declaration.
    /// Used by TS2838 validation to check that duplicate infer names have identical constraints.
    pub(crate) fn collect_infer_type_params_with_constraints(
        &self,
        type_idx: NodeIndex,
    ) -> Vec<(String, NodeIndex, NodeIndex)> {
        let mut params = Vec::new();
        self.collect_infer_params_with_constraints_inner(type_idx, &mut params);
        params
    }

    fn collect_infer_params_with_constraints_inner(
        &self,
        type_idx: NodeIndex,
        params: &mut Vec<(String, NodeIndex, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node)
                    && let Some(param_node) = self.ctx.arena.get(infer.type_parameter)
                    && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                    && let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    params.push((
                        ident.escaped_text.clone(),
                        param.constraint,
                        infer.type_parameter,
                    ));
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(ref args) = type_ref.type_arguments
                {
                    for &arg_idx in &args.nodes {
                        self.collect_infer_params_with_constraints_inner(arg_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.collect_infer_params_with_constraints_inner(member_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    if let Some(ref tps) = func_type.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_params_with_constraints_inner(tp_idx, params);
                        }
                    }
                    for &param_idx in &func_type.parameters.nodes {
                        self.collect_infer_params_with_constraints_inner(param_idx, params);
                    }
                    if func_type.type_annotation.is_some() {
                        self.collect_infer_params_with_constraints_inner(
                            func_type.type_annotation,
                            params,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                    self.collect_infer_params_with_constraints_inner(
                        array_type.element_type,
                        params,
                    );
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple_type.elements.nodes {
                        self.collect_infer_params_with_constraints_inner(elem_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.collect_infer_params_with_constraints_inner(member_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.collect_infer_params_with_constraints_inner(op.type_node, params);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.collect_infer_params_with_constraints_inner(indexed.object_type, params);
                    self.collect_infer_params_with_constraints_inner(indexed.index_type, params);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.collect_infer_params_with_constraints_inner(mapped.type_parameter, params);
                    if mapped.type_node.is_some() {
                        self.collect_infer_params_with_constraints_inner(mapped.type_node, params);
                    }
                    if mapped.name_type.is_some() {
                        self.collect_infer_params_with_constraints_inner(mapped.name_type, params);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    self.collect_infer_params_with_constraints_inner(cond.check_type, params);
                    self.collect_infer_params_with_constraints_inner(cond.extends_type, params);
                    self.collect_infer_params_with_constraints_inner(cond.true_type, params);
                    self.collect_infer_params_with_constraints_inner(cond.false_type, params);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        self.collect_infer_params_with_constraints_inner(span_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN => {
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    self.collect_infer_params_with_constraints_inner(span.expression, params);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_infer_params_with_constraints_inner(wrapped.expression, params);
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE || k == syntax_kind_ext::REST_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.collect_infer_params_with_constraints_inner(wrapped.type_node, params);
                }
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.ctx.arena.get_named_tuple_member(node) {
                    self.collect_infer_params_with_constraints_inner(member.type_node, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(type_param) = self.ctx.arena.get_type_parameter(node) {
                    if type_param.constraint != NodeIndex::NONE {
                        self.collect_infer_params_with_constraints_inner(
                            type_param.constraint,
                            params,
                        );
                    }
                    if type_param.default != NodeIndex::NONE {
                        self.collect_infer_params_with_constraints_inner(
                            type_param.default,
                            params,
                        );
                    }
                }
            }
            _ => {
                if let Some(sig) = self.ctx.arena.get_signature(node) {
                    if let Some(ref tps) = sig.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_params_with_constraints_inner(tp_idx, params);
                        }
                    }
                    if let Some(ref sig_params) = sig.parameters {
                        for &param_idx in &sig_params.nodes {
                            self.collect_infer_params_with_constraints_inner(param_idx, params);
                        }
                    }
                    if sig.type_annotation.is_some() {
                        self.collect_infer_params_with_constraints_inner(
                            sig.type_annotation,
                            params,
                        );
                    }
                } else if let Some(index_sig) = self.ctx.arena.get_index_signature(node) {
                    for &param_idx in &index_sig.parameters.nodes {
                        self.collect_infer_params_with_constraints_inner(param_idx, params);
                    }
                    if index_sig.type_annotation.is_some() {
                        self.collect_infer_params_with_constraints_inner(
                            index_sig.type_annotation,
                            params,
                        );
                    }
                } else if let Some(param) = self.ctx.arena.get_parameter(node)
                    && param.type_annotation != NodeIndex::NONE
                {
                    self.collect_infer_params_with_constraints_inner(param.type_annotation, params);
                }
            }
        }
    }

    /// Inner implementation for collecting infer type parameters.
    /// Recursively walks the type node to find all infer type parameter names.
    fn collect_infer_type_parameters_inner(&self, type_idx: NodeIndex, params: &mut Vec<String>) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node)
                    && let Some(param_node) = self.ctx.arena.get(infer.type_parameter)
                    && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                    && let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = ident.escaped_text.clone();
                    if !params.contains(&name) {
                        params.push(name);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(ref args) = type_ref.type_arguments
                {
                    for &arg_idx in &args.nodes {
                        self.collect_infer_type_parameters_inner(arg_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            // Function and Constructor Types: check parameters and return type
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    // Check type parameters (they may have infer in constraints)
                    if let Some(ref tps) = func_type.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_type_parameters_inner(tp_idx, params);
                        }
                    }
                    // Check parameters
                    for &param_idx in &func_type.parameters.nodes {
                        self.collect_infer_type_parameters_inner(param_idx, params);
                    }
                    // Check return type
                    if func_type.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(func_type.type_annotation, params);
                    }
                }
            }
            // Array Types: check element type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                    self.collect_infer_type_parameters_inner(array_type.element_type, params);
                }
            }
            // Tuple Types: check all elements
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple_type) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple_type.elements.nodes {
                        self.collect_infer_type_parameters_inner(elem_idx, params);
                    }
                }
            }
            // Type Literals (Object types): check all members
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            // Type Operators: keyof, readonly, unique - check operand
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.collect_infer_type_parameters_inner(op.type_node, params);
                }
            }
            // Indexed Access Types: T[K] - check both object and index
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.collect_infer_type_parameters_inner(indexed.object_type, params);
                    self.collect_infer_type_parameters_inner(indexed.index_type, params);
                }
            }
            // Mapped Types: check type parameter (constraint) and type template
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.collect_infer_type_parameters_inner(mapped.type_parameter, params);
                    if mapped.type_node.is_some() {
                        self.collect_infer_type_parameters_inner(mapped.type_node, params);
                    }
                    if mapped.name_type.is_some() {
                        self.collect_infer_type_parameters_inner(mapped.name_type, params);
                    }
                }
            }
            // Conditional Types: check check_type, extends_type, true_type, false_type
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    self.collect_infer_type_parameters_inner(cond.check_type, params);
                    self.collect_infer_type_parameters_inner(cond.extends_type, params);
                    self.collect_infer_type_parameters_inner(cond.true_type, params);
                    self.collect_infer_type_parameters_inner(cond.false_type, params);
                }
            }
            // Template Literal Types: check all type spans
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        self.collect_infer_type_parameters_inner(span_idx, params);
                    }
                }
            }
            // Template Literal Type Spans: recurse into the type expression
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN => {
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    self.collect_infer_type_parameters_inner(span.expression, params);
                }
            }
            // Parenthesized Types: unwrap and check inner type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_infer_type_parameters_inner(wrapped.expression, params);
                }
            }
            // Optional, Rest Types: unwrap and check inner type
            k if k == syntax_kind_ext::OPTIONAL_TYPE || k == syntax_kind_ext::REST_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.collect_infer_type_parameters_inner(wrapped.type_node, params);
                }
            }
            // Named Tuple Members: check the type annotation
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.ctx.arena.get_named_tuple_member(node) {
                    self.collect_infer_type_parameters_inner(member.type_node, params);
                }
            }
            // Type Parameters: check constraint and default for nested infer
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(type_param) = self.ctx.arena.get_type_parameter(node) {
                    // Check constraint: <T extends infer U>
                    if type_param.constraint != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(type_param.constraint, params);
                    }
                    // Check default: <T = infer U>
                    if type_param.default != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(type_param.default, params);
                    }
                }
            }
            _ => {
                // Signatures (PropertySignature, MethodSignature, CallSignature, ConstructSignature):
                // recurse into type parameters, parameters, and return type
                if let Some(sig) = self.ctx.arena.get_signature(node) {
                    if let Some(ref tps) = sig.type_parameters {
                        for &tp_idx in &tps.nodes {
                            self.collect_infer_type_parameters_inner(tp_idx, params);
                        }
                    }
                    if let Some(ref sig_params) = sig.parameters {
                        for &param_idx in &sig_params.nodes {
                            self.collect_infer_type_parameters_inner(param_idx, params);
                        }
                    }
                    if sig.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(sig.type_annotation, params);
                    }
                } else if let Some(index_sig) = self.ctx.arena.get_index_signature(node) {
                    // IndexSignature: recurse into parameters and type annotation
                    for &param_idx in &index_sig.parameters.nodes {
                        self.collect_infer_type_parameters_inner(param_idx, params);
                    }
                    if index_sig.type_annotation.is_some() {
                        self.collect_infer_type_parameters_inner(index_sig.type_annotation, params);
                    }
                } else if let Some(param) = self.ctx.arena.get_parameter(node) {
                    // Parameters: check the type annotation
                    if param.type_annotation != NodeIndex::NONE {
                        self.collect_infer_type_parameters_inner(param.type_annotation, params);
                    }
                }
            }
        }
    }

    // Section 40: Node and Name Utilities
    // ------------------------------------

    /// Get the text content of a node from the source file.
    pub(crate) fn node_text(&self, node_idx: NodeIndex) -> Option<String> {
        let (start, end) = self.get_node_span(node_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let start = start as usize;
        let end = end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        Some(source[start..end].to_string())
    }

    /// Get the name of a parameter for error messages.
    pub(crate) fn parameter_name_for_error(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return "this".to_string();
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                return lit.text.clone();
            }
        }

        self.node_text(name_idx)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "parameter".to_string())
    }

    /// Get the name of a property for error messages.
    pub(crate) fn property_name_for_error(&self, name_idx: NodeIndex) -> Option<String> {
        self.get_property_name(name_idx).or_else(|| {
            self.node_text(name_idx)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        })
    }

    /// Collect all nodes within an initializer expression that reference a given name.
    /// Used for TS2372: parameter cannot reference itself.
    ///
    /// Recursively walks the initializer AST and collects every identifier node
    /// that matches `name`. Stops recursion at scope boundaries (function expressions,
    /// arrow functions, class expressions) since those introduce new scopes where
    /// the identifier would not be a self-reference of the outer parameter.
    ///
    /// Returns a list of `NodeIndex` values, one for each self-referencing identifier.
    /// TSC emits a separate TS2372 error for each occurrence.
    pub(crate) fn collect_self_references(
        &self,
        init_idx: NodeIndex,
        name: &str,
    ) -> Vec<NodeIndex> {
        let mut refs = Vec::new();
        self.collect_self_references_recursive(init_idx, name, &mut refs);
        refs
    }

    /// Collect property-access occurrences whose property name matches `name`.
    ///
    /// This is used for accessor recursion detection (TS7023). It intentionally
    /// ignores bare identifiers so captured outer variables like `return x` in
    /// `get x() { ... }` are not treated as self-recursive references.
    pub(crate) fn collect_property_name_references(
        &self,
        init_idx: NodeIndex,
        name: &str,
    ) -> Vec<NodeIndex> {
        let mut refs = Vec::new();
        self.collect_property_name_references_recursive(init_idx, name, &mut refs);
        refs
    }

    /// Recursive helper for `collect_self_references`.
    fn collect_self_references_recursive(
        &self,
        node_idx: NodeIndex,
        name: &str,
        refs: &mut Vec<NodeIndex>,
    ) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // If this node is an identifier matching the parameter name, record it
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            if ident.escaped_text == name {
                refs.push(node_idx);
            }
            return;
        }

        // Stop at scope boundaries: function expressions, arrow functions,
        // and class expressions introduce new scopes where the name would
        // refer to something different (not the outer parameter).
        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => {
                return;
            }
            _ => {}
        }

        // Recurse into all children of this node
        let children = self.ctx.arena.get_children(node_idx);
        for child_idx in children {
            self.collect_self_references_recursive(child_idx, name, refs);
        }
    }

    /// Recursive helper for `collect_property_name_references`.
    fn collect_property_name_references_recursive(
        &self,
        node_idx: NodeIndex,
        name: &str,
        refs: &mut Vec<NodeIndex>,
    ) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text == name
        {
            refs.push(access.name_or_argument);
        }

        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => {
                return;
            }
            _ => {}
        }

        let children = self.ctx.arena.get_children(node_idx);
        for child_idx in children {
            self.collect_property_name_references_recursive(child_idx, name, refs);
        }
    }

    /// Collect `this.foo` property accesses that occur within return expressions,
    /// excluding nested deferred boundaries.
    pub(crate) fn collect_return_expression_this_property_accesses(
        &self,
        body_idx: NodeIndex,
    ) -> Vec<(NodeIndex, String)> {
        let mut refs = Vec::new();
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return refs;
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                for &stmt_idx in &block.statements.nodes {
                    self.collect_this_property_accesses_in_return_statement(stmt_idx, &mut refs);
                }
            }
        } else {
            self.collect_this_property_accesses_in_expression(body_idx, &mut refs);
        }

        refs
    }

    fn collect_this_property_accesses_in_return_statement(
        &self,
        stmt_idx: NodeIndex,
        refs: &mut Vec<(NodeIndex, String)>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.collect_this_property_accesses_in_expression(ret.expression, refs);
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_this_property_accesses_in_return_statement(stmt, refs);
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    self.collect_this_property_accesses_in_return_statement(
                        if_stmt.then_statement,
                        refs,
                    );
                    if if_stmt.else_statement.is_some() {
                        self.collect_this_property_accesses_in_return_statement(
                            if_stmt.else_statement,
                            refs,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_stmt) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_stmt.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_this_property_accesses_in_return_statement(stmt, refs);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.ctx.arena.get_try(node) {
                    self.collect_this_property_accesses_in_return_statement(
                        try_stmt.try_block,
                        refs,
                    );
                    if try_stmt.catch_clause.is_some() {
                        self.collect_this_property_accesses_in_return_statement(
                            try_stmt.catch_clause,
                            refs,
                        );
                    }
                    if try_stmt.finally_block.is_some() {
                        self.collect_this_property_accesses_in_return_statement(
                            try_stmt.finally_block,
                            refs,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_clause) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_this_property_accesses_in_return_statement(
                        catch_clause.block,
                        refs,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_stmt) = self.ctx.arena.get_loop(node) {
                    self.collect_this_property_accesses_in_return_statement(
                        loop_stmt.statement,
                        refs,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_stmt) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_this_property_accesses_in_return_statement(
                        loop_stmt.statement,
                        refs,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_this_property_accesses_in_return_statement(
                        labeled.statement,
                        refs,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_this_property_accesses_in_expression(
        &self,
        node_idx: NodeIndex,
        refs: &mut Vec<(NodeIndex, String)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && let Some(receiver_node) = self.ctx.arena.get(access.expression)
            && receiver_node.kind == SyntaxKind::ThisKeyword as u16
            && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            refs.push((access.name_or_argument, ident.escaped_text.clone()));
        }

        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => return,
            _ => {}
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.collect_this_property_accesses_in_expression(child_idx, refs);
        }
    }

    // Section 41: Function Implementation Checking
    // --------------------------------------------

    /// Infer the return type of a getter from its body.
    pub(crate) fn infer_getter_return_type(&mut self, body_idx: NodeIndex) -> TypeId {
        self.infer_return_type_from_body(tsz_parser::parser::NodeIndex::NONE, body_idx, None)
    }

    /// Check that all top-level function overload signatures have implementations.
    /// Reports errors 2389, 2391.
    pub(crate) fn check_function_implementations(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < statements.len() {
            let stmt_idx = statements[i];
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                i += 1;
                continue;
            };

            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func) = self.ctx.arena.get_function(node)
                && func.body.is_none()
            {
                // Suppress TS2391 when a parse error occurs within the function declaration span.
                // When `body.is_none()` and there are parse errors within the function span,
                // the function was likely malformed (e.g. `function f() => 4;`).
                // This doesn't affect cases like `function f(a {` because the parser gives
                // those a body (`body_none=false`) so they never reach this path.
                if self.has_syntax_parse_errors() {
                    let fn_start = node.pos;
                    let fn_end = node.end;
                    let has_error_in_fn = self
                        .ctx
                        .syntax_parse_error_positions
                        .iter()
                        .any(|&p| p >= fn_start && p <= fn_end);
                    if has_error_in_fn {
                        i += 1;
                        continue;
                    }
                }
                let is_declared = self.is_ambient_declaration(stmt_idx);
                // Use func.is_async as the parser stores async as a flag, not a modifier
                let is_async = func.is_async;
                // TSC reports TS2389/TS2391 at the function name, not the declaration.
                let name_node = func.name;
                let error_node = if name_node.is_some() {
                    name_node
                } else {
                    stmt_idx
                };

                // TS1040: 'async' modifier cannot be used in an ambient context
                // The parser emits TS1040 at the 'async' keyword for both
                // top-level `declare async function` and class member async
                // methods in ambient context. Skip the checker's duplicate.
                if is_declared && is_async {
                    i += 1;
                    continue;
                }

                if is_declared {
                    if let Some(name) = self.get_function_name_from_node(stmt_idx) {
                        let (has_impl, impl_name, impl_idx) =
                            self.find_function_impl(statements, i + 1, &name);
                        if has_impl
                            && impl_name.as_deref() == Some(name.as_str())
                            && let Some(impl_idx) = impl_idx
                            && !self.is_ambient_declaration(impl_idx)
                        {
                            self.error_at_node(
                                error_node,
                                crate::diagnostics::diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                                crate::diagnostics::diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                            );
                        }
                    }
                    i += 1;
                    continue;
                }
                if is_async {
                    i += 1;
                    continue;
                }
                // Function overload signature - check for implementation.
                // TSC only reports TS2391 on the LAST overload in a consecutive
                // group with the same name, so skip ahead to find it.
                let func_name = self.get_function_name_from_node(stmt_idx);
                if let Some(name) = func_name {
                    // Advance past consecutive bodyless overloads with the same name.
                    let mut last_overload_i = i;
                    let mut j = i + 1;
                    while j < statements.len() {
                        let next_idx = statements[j];
                        let Some(next_node) = self.ctx.arena.get(next_idx) else {
                            break;
                        };
                        if next_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                            && let Some(next_func) = self.ctx.arena.get_function(next_node)
                            && next_func.body.is_none()
                        {
                            let next_name = self.get_function_name_from_node(next_idx);
                            if next_name.as_deref() == Some(name.as_str()) {
                                last_overload_i = j;
                                j += 1;
                                continue;
                            }
                        }
                        break;
                    }

                    // Report at the last overload in the group
                    let report_stmt_idx = statements[last_overload_i];
                    let report_error_node = self
                        .ctx
                        .arena
                        .get(report_stmt_idx)
                        .and_then(|n| self.ctx.arena.get_function(n))
                        .map(|f| f.name)
                        .filter(|n| n.is_some())
                        .unwrap_or(report_stmt_idx);

                    let (has_impl, impl_name, impl_idx) =
                        self.find_function_impl(statements, last_overload_i + 1, &name);
                    if !has_impl {
                        self.error_at_node(
                                    report_error_node,
                                    "Function implementation is missing or not immediately following the declaration.",
                                    diagnostic_codes::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION
                                );
                    } else if let Some(impl_idx) = impl_idx {
                        if let Some(actual_name) = impl_name
                            && actual_name != name
                        {
                            // Implementation has wrong name — report at the implementation name.
                            let impl_error_node = self
                                .ctx
                                .arena
                                .get(impl_idx)
                                .and_then(|n| self.ctx.arena.get_function(n))
                                .map(|f| f.name)
                                .filter(|n| n.is_some())
                                .unwrap_or(impl_idx);
                            self.error_at_node(
                                impl_error_node,
                                &format!("Function implementation name must be '{name}'."),
                                diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                            );
                        } else {
                            let impl_is_declared = self.is_ambient_declaration(impl_idx);
                            if is_declared != impl_is_declared {
                                self.error_at_node(
                                    report_error_node,
                                    crate::diagnostics::diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                                    crate::diagnostics::diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                                );
                            }
                        }
                    }
                    // Skip past all overloads we already processed
                    i = last_overload_i + 1;
                    continue;
                }
            }
            i += 1;
        }
    }

    // Section 42: Class Member Utilities
    // ------------------------------------

    /// Check if a class member is static.
    pub(crate) fn class_member_is_static(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .is_some_and(|prop| self.has_static_modifier(&prop.modifiers)),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|method| self.has_static_modifier(&method.modifiers)),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .is_some_and(|accessor| self.has_static_modifier(&accessor.modifiers)),
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => true,
            _ => false,
        }
    }

    /// Get the declaring type for a private member.
    pub(crate) fn private_member_declaring_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if !matches!(
                node.kind,
                k if k == syntax_kind_ext::PROPERTY_DECLARATION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
            ) {
                continue;
            }

            let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
                continue;
            };
            if ext.parent.is_none() {
                continue;
            }
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                continue;
            };
            if parent_node.kind != syntax_kind_ext::CLASS_DECLARATION
                && parent_node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(parent_node) else {
                continue;
            };
            let is_static = self.class_member_is_static(decl_idx);
            return Some(if is_static {
                self.get_class_constructor_type(ext.parent, class)
            } else {
                self.get_class_instance_type(ext.parent, class)
            });
        }

        None
    }

    /// Check if a type annotation node is a simple type reference to a given class.
    /// Returns true if the type annotation is a `TypeReference` to the class by name.
    fn type_annotation_refers_to_current_class(
        &self,
        type_annotation_idx: NodeIndex,
        class_idx: NodeIndex,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_annotation_idx) else {
            return false;
        };

        // Check if it's a type reference
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }

        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return false;
        };

        // Get the name from the type reference
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };

        let type_ref_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            &ident.escaped_text
        } else {
            return false;
        };

        // Get the class name
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };

        let Some(class) = self.ctx.arena.get_class(class_node) else {
            return false;
        };

        if class.name.is_none() {
            return false;
        }

        let Some(class_name_node) = self.ctx.arena.get(class.name) else {
            return false;
        };

        let class_name = if let Some(ident) = self.ctx.arena.get_identifier(class_name_node) {
            &ident.escaped_text
        } else {
            return false;
        };

        // Compare names
        type_ref_name == class_name
    }

    /// Get the type annotation of an explicit `this` parameter if present.
    /// Returns `Some(type_annotation_idx)` if the first parameter is named "this" with a type annotation.
    /// Returns None otherwise.
    fn get_explicit_this_type_annotation(&self, params: &[NodeIndex]) -> Option<NodeIndex> {
        let first_param_idx = params.first().copied()?;
        let param_node = self.ctx.arena.get(first_param_idx)?;
        let param = self.ctx.arena.get_parameter(param_node)?;

        // Check if parameter name is "this"
        // Must check both ThisKeyword and Identifier("this") to match parser behavior
        let is_this = if let Some(name_node) = self.ctx.arena.get(param.name) {
            if name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
                true
            } else if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text == "this"
            } else {
                false
            }
        } else {
            false
        };

        // Explicit `this` parameter must have a type annotation
        if is_this {
            param.type_annotation.into_option()
        } else {
            None
        }
    }

    /// Get the this type for a class member.
    pub(crate) fn class_member_this_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let class_idx = class_info.class_idx;
        let cached_instance_this = class_info.cached_instance_this_type;
        let is_static = self.class_member_is_static(member_idx);

        // Check if this method/accessor has an explicit `this` parameter.
        // If so, extract and return its type instead of the default class type.
        if let Some(node) = self.ctx.arena.get(member_idx) {
            let (explicit_this_type_annotation, member_type_params) = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        (
                            self.get_explicit_this_type_annotation(&method.parameters.nodes),
                            method.type_parameters.clone(),
                        )
                    } else {
                        (None, None)
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                        (
                            self.get_explicit_this_type_annotation(&accessor.parameters.nodes),
                            accessor.type_parameters.clone(),
                        )
                    } else {
                        (None, None)
                    }
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                        (
                            self.get_explicit_this_type_annotation(&accessor.parameters.nodes),
                            accessor.type_parameters.clone(),
                        )
                    } else {
                        (None, None)
                    }
                }
                _ => (None, None),
            };

            if let Some(type_annotation_idx) = explicit_this_type_annotation {
                // Check if the explicit `this` type refers to the current class.
                // If so, we should use the cached instance type to avoid resolution timing issues.
                let refers_to_current_class =
                    self.type_annotation_refers_to_current_class(type_annotation_idx, class_idx);

                if refers_to_current_class && !is_static {
                    // For instance methods with `this: CurrentClass`, use the cached instance type
                    // This ensures we get the fully-constructed class type with all properties
                    if let Some(cached) = cached_instance_this {
                        return Some(cached);
                    }
                    if let Some(node) = self.ctx.arena.get(class_idx)
                        && let Some(class) = self.ctx.arena.get_class(node)
                    {
                        return Some(self.get_class_instance_type(class_idx, class));
                    }
                }

                // Push method's own type parameters into scope before resolving
                // the `this` type annotation. Without this, `this: T` where `T` is
                // the method's type parameter would fail with TS2304.
                let (_tp, tp_updates) = self.push_type_parameters(&member_type_params);
                let explicit_this_type = self.get_type_from_type_node(type_annotation_idx);
                self.pop_type_parameters(tp_updates);
                return Some(explicit_this_type);
            }
        }

        if !is_static {
            if let Some(cached) = cached_instance_this {
                return Some(cached);
            }

            if let Some(sym_id) = self.ctx.binder.get_node_symbol(class_idx)
                && let Some(instance_type) = self.class_instance_type_from_symbol(sym_id)
            {
                if instance_type != TypeId::ERROR {
                    if let Some(info) = self.ctx.enclosing_class.as_mut()
                        && info.class_idx == class_idx
                    {
                        info.cached_instance_this_type = Some(instance_type);
                    }
                    return Some(instance_type);
                }
                tracing::debug!(
                    class_sym = sym_id.0,
                    "class_member_this_type: symbol fallback produced ERROR"
                );
            }

            // Use the current class type parameters in scope for instance `this`.
            if let Some(node) = self.ctx.arena.get(class_idx)
                && let Some(class) = self.ctx.arena.get_class(node)
            {
                let this_type = self.get_class_instance_type(class_idx, class);
                if let Some(info) = self.ctx.enclosing_class.as_mut()
                    && info.class_idx == class_idx
                {
                    info.cached_instance_this_type = Some(this_type);
                }
                return Some(this_type);
            }
        }

        // For static members, `this` is the constructor type (`typeof A`), not the
        // instance type. `get_type_of_symbol` on a class symbol returns the instance
        // type, so we must use `get_class_constructor_type` explicitly.
        if is_static {
            let class = self.ctx.arena.get_class_at(class_idx)?;
            return Some(self.get_class_constructor_type(class_idx, class));
        }

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(class_idx) {
            return self.class_instance_type_from_symbol(sym_id);
        }

        let class = self.ctx.arena.get_class_at(class_idx)?;
        Some(self.get_class_instance_type(class_idx, class))
    }

    // Section 43: Accessor Type Checking
    // -----------------------------------

    /// Recursively check for TS7006 in nested function/arrow expressions within a node.
    /// This handles cases like `async function foo(a = x => x)` where the nested arrow function
    /// parameter `x` should trigger TS7006 if it lacks a type annotation.
    pub(crate) fn check_for_nested_function_ts7006(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Check if this is a function or arrow expression
        let is_function = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => true,
            k if k == syntax_kind_ext::ARROW_FUNCTION => true,
            _ => false,
        };

        if is_function {
            // Check all parameters of this function for TS7006
            if let Some(func) = self.ctx.arena.get_function(node) {
                for (pi, &param_idx) in func.parameters.nodes.iter().enumerate() {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        // Nested functions in default values don't have contextual types
                        self.maybe_report_implicit_any_parameter(param, false, pi);
                    }
                }
            }

            // Recursively check the function body for more nested functions
            if let Some(func) = self.ctx.arena.get_function(node)
                && func.body.is_some()
            {
                self.check_for_nested_function_ts7006(func.body);
            }
        } else {
            // Recursively check child nodes for function expressions
            match node.kind {
                // Binary expressions - check both sides
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                        self.check_for_nested_function_ts7006(bin_expr.left);
                        self.check_for_nested_function_ts7006(bin_expr.right);
                    }
                }
                // Conditional expressions - check condition, then/else branches
                k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                        self.check_for_nested_function_ts7006(cond.condition);
                        self.check_for_nested_function_ts7006(cond.when_true);
                        if cond.when_false.is_some() {
                            self.check_for_nested_function_ts7006(cond.when_false);
                        }
                    }
                }
                // Call expressions - only check the callee, NOT arguments.
                // Arguments to call expressions get proper contextual types from
                // the call resolution path (collect_call_argument_types_with_context),
                // so arrow/function expressions in arguments will have their TS7006
                // correctly suppressed by the contextual type. Walking arguments here
                // would emit false TS7006 before contextual typing has a chance to run.
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call) = self.ctx.arena.get_call_expr(node) {
                        self.check_for_nested_function_ts7006(call.expression);
                    }
                }
                // New expressions - same treatment: only check the callee, skip arguments
                // since constructor resolution provides contextual types for arguments.
                k if k == syntax_kind_ext::NEW_EXPRESSION => {
                    if let Some(new_expr) = self.ctx.arena.get_call_expr(node) {
                        self.check_for_nested_function_ts7006(new_expr.expression);
                    }
                }
                // Parenthesized expression - check contents
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                        self.check_for_nested_function_ts7006(paren.expression);
                    }
                }
                // Type assertion - check expression
                k if k == syntax_kind_ext::TYPE_ASSERTION => {
                    if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                        self.check_for_nested_function_ts7006(assertion.expression);
                    }
                }
                // Spread element - check expression
                k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                    if let Some(spread) = self.ctx.arena.get_spread(node) {
                        self.check_for_nested_function_ts7006(spread.expression);
                    }
                }
                _ => {
                    // For other node types, we don't recursively check
                    // This covers literals, identifiers, array/object literals, etc.
                }
            }
        }
    }
}
