//! Type checking query helpers: type parameter scope, function implementation
//! checking, and class member analysis.
//!
//! Library type resolution (`resolve_lib_type_by_name`, `merge_lib_interface_heritage`)
//! has been extracted to `queries/lib_resolution.rs`.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, SourceFileData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Section 39: Type Parameter Scope Utilities
    // =========================================================================

    /// Resolve declaration-ordered type-parameter infos to the exact identities
    /// currently installed in the checker scope. Returns `None` rather than a
    /// partial vector so callers never shift positional binder alignment.
    pub(crate) fn exact_type_parameter_ids_in_scope(
        &self,
        params: &[tsz_solver::TypeParamInfo],
    ) -> Option<Vec<TypeId>> {
        params
            .iter()
            .map(|param| {
                let name = self.ctx.types.resolve_atom_ref(param.name);
                self.ctx.type_parameter_scope.get(name.as_ref()).copied()
            })
            .collect()
    }

    /// Exact type-parameter identities available to a member expression.
    ///
    /// The name-keyed scope contains the innermost binder for each spelling,
    /// so a method parameter can hide an enclosing class parameter there. A
    /// cached class member still carries the enclosing binder's exact identity;
    /// retain both identities when deciding which free parameters are bound.
    pub(crate) fn member_type_parameter_ids_in_scope(&self) -> rustc_hash::FxHashSet<TypeId> {
        let mut in_scope = self
            .ctx
            .type_parameter_scope
            .values()
            .copied()
            .collect::<rustc_hash::FxHashSet<_>>();
        if let Some(class_info) = self.ctx.enclosing_class.as_ref() {
            in_scope.extend(class_info.class_type_parameter_ids.iter().copied());
        }
        in_scope
    }

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

    /// Populate `type_param_constraint_excluded_params` with the names of all
    /// value parameters in the given parameter list. This prevents `typeof paramName`
    /// from resolving those parameters while processing type parameter constraints.
    pub(crate) fn exclude_params_for_type_param_constraints(
        &mut self,
        params: &tsz_parser::parser::base::NodeList,
    ) {
        for &param_idx in &params.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                self.collect_param_names_into_exclusion(param.name);
            }
        }
    }

    /// Recursively collect binding names from a parameter name (handles identifiers,
    /// object binding patterns, and array binding patterns).
    fn collect_param_names_into_exclusion(&mut self, name_idx: tsz_parser::parser::NodeIndex) {
        let Some(node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            self.ctx
                .type_param_constraint_excluded_params
                .insert(ident.escaped_text.to_string());
            return;
        }
        if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
            for &elem_idx in &pattern.elements.nodes {
                if let Some(elem_node) = self.ctx.arena.get(elem_idx)
                    && let Some(elem) = self.ctx.arena.get_binding_element(elem_node)
                {
                    self.collect_param_names_into_exclusion(elem.name);
                }
            }
        }
    }

    /// Clear excluded parameter names after type parameter constraints have been processed.
    pub(crate) fn clear_excluded_params_for_type_param_constraints(&mut self) {
        self.ctx.type_param_constraint_excluded_params.clear();
    }

    /// Check for unused type parameters in a declaration and emit TS6196.
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
        let mut params: Vec<(String, NodeIndex, NodeIndex)> = Vec::new();
        for &param_idx in &list.nodes {
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
                .map(|id_data| id_data.escaped_text.to_string())
                .unwrap_or_default();
            if !name.is_empty() && !name.starts_with('_') {
                params.push((name, data.name, param_idx));
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

        // Determine if this declaration is part of a cross-file merge. TypeScript
        // 7 does not report unused type parameters when a merged symbol spans
        // source files: the declaration-local usage walk is not authoritative
        // once the type-parameter surface is assembled from multiple files.
        let mut is_cross_file_merge = false;

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
        }

        if is_cross_file_merge {
            return;
        }

        let decl_indices: Vec<NodeIndex> =
            params.iter().map(|(_, name_idx, _)| *name_idx).collect();
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

        // Emit TS6196 for unused type parameters.
        for (j, (name, _name_idx, param_idx)) in params.iter().enumerate() {
            if used[j] {
                continue;
            }
            let Some(param_node) = self.ctx.arena.get(*param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let start = param
                .modifiers
                .as_ref()
                .and_then(|modifiers| modifiers.nodes.first())
                .and_then(|modifier| self.ctx.arena.get(*modifier))
                .map_or_else(
                    || {
                        self.ctx
                            .arena
                            .get(param.name)
                            .map_or(param_node.pos, |node| node.pos)
                    },
                    |node| node.pos,
                );
            let end = [param.default, param.constraint, param.name]
                .into_iter()
                .find_map(|idx| self.ctx.arena.get(idx).map(|node| node.end))
                .unwrap_or(param_node.end);
            self.error_declared_but_never_used(name, start, end.saturating_sub(start));
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

        // Underscore-prefixed parameters are exempt from unused diagnostics and
        // therefore count as referenced for the all-unused aggregate decision.
        let mut used: Vec<bool> = params.iter().map(|param| param.4).collect();
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
                for (j, (param_name, _, _, _, _)) in params.iter().enumerate() {
                    if !used[j] && param_name == name_str {
                        used[j] = true;
                    }
                }
            }
        }

        for type_expr in Self::jsdoc_type_expressions(raw_comment) {
            for (j, (param_name, _, _, _, _)) in params.iter().enumerate() {
                if !used[j] && Self::jsdoc_type_expr_mentions_name(type_expr, param_name) {
                    used[j] = true;
                }
            }
        }

        // TypeScript 7 flattens every `@template` tag attached to this
        // declaration into one type-parameter list before checking usage.
        if params.len() > 1 && used.iter().all(|is_used| !is_used) {
            let start = params[0].3.saturating_sub(1);
            let end = node.pos;
            self.error_all_type_parameters_unused(start, end.saturating_sub(start));
        } else {
            for (j, (name, name_pos, name_len, _, _)) in params.iter().enumerate() {
                if !used[j] {
                    self.error_declared_but_never_used(name, *name_pos, *name_len);
                }
            }
        }
    }

    /// Returns `(name, name_pos, name_length, tag_start, exempt)` for each JSDoc
    /// `@template` parameter in `raw_comment`. `tag_start` preserves the first
    /// list anchor after TypeScript 7 flattens all tags on the declaration;
    /// `exempt` records the underscore convention for TS6205/TS6196 selection.
    fn jsdoc_template_param_declarations(
        raw_comment: &str,
        comment_pos: u32,
    ) -> Vec<(String, u32, u32, u32, bool)> {
        let mut params = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel) = Self::jsdoc_tag_offset(&raw_comment[cursor..], "template") {
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
                    let name_pos = comment_pos + start as u32;
                    let name_len = (idx - start) as u32;
                    let tag_abs = comment_pos + tag_start as u32;
                    params.push((
                        name.to_string(),
                        name_pos,
                        name_len,
                        tag_abs,
                        name.starts_with('_'),
                    ));
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
        const fn is_ident_char(ch: char) -> bool {
            tsz_common::text_scan::is_ascii_identifier_continue_char(ch)
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

    // Section 40: Node and Name Utilities
    // ------------------------------------

    /// Get the text content of a node from the source file that actually owns
    /// it, not the arena's first source file — an arena can hold more than one
    /// file (e.g. a lib-merged declaration), so `.first()` silently slices the
    /// wrong file's text whenever `node_idx` isn't in it.
    pub(crate) fn node_text(&self, node_idx: NodeIndex) -> Option<String> {
        let (start, end) = self.get_node_span(node_idx)?;
        let source = self.owning_source_file(node_idx)?.text.as_ref();
        let start = start as usize;
        let end = end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        Some(source[start..end].to_string())
    }

    /// Walk up from `node_idx` to its enclosing `SOURCE_FILE` node and return
    /// that file's data.
    fn owning_source_file(&self, node_idx: NodeIndex) -> Option<&SourceFileData> {
        let mut current = node_idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::SOURCE_FILE {
                return self.ctx.arena.get_source_file(node);
            }
            let info = self.ctx.arena.node_info(current)?;
            if info.parent.is_none() {
                return None;
            }
            current = info.parent;
        }
        None
    }

    /// Get the name of a parameter for error messages.
    ///
    /// Also reused by callers that pass an accessor's *property* name
    /// (`set "foo"(v)`'s TS7032 site), so a string-literal name must keep its
    /// source quote character rather than the raw unquoted `lit.text`.
    pub(crate) fn parameter_name_for_error(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return "this".to_string();
            }
            if (name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                || name_node.kind == SyntaxKind::StringLiteral as u16)
                && let Some(display_name) = self.get_member_name_display_text(name_idx)
            {
                return display_name;
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text.to_string();
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
        self.member_name_for_diagnostic(name_idx).or_else(|| {
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

    /// Whether an object-literal get-accessor body contains an *indirect
    /// self-reference* through a property access, the signal for TS7023.
    ///
    /// A `<receiver>.<name>` access (where `<name>` is the accessor's own name)
    /// is a self-reference only when `<receiver>` evaluates to the object
    /// literal currently under construction: the synthetic `this`, the variable
    /// the literal initializes, or a transparent wrapper / index / conditional
    /// of those. `tsc` resolves the receiver's actual member symbol, so a
    /// `.<name>` access on an *unrelated* receiver (`ctx.path`, `mgr.clients`)
    /// reads a different member's symbol and is not circular — it must not
    /// trigger TS7023. A bare property-*name* match is not a sufficient signal;
    /// this mirrors the `this`-receiver gate used for object-literal
    /// method-call cycles (`object_literal_circularity.rs`).
    pub(crate) fn object_literal_getter_has_self_reference(
        &mut self,
        accessor_idx: NodeIndex,
        body_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let binding_sym = self.object_literal_initializer_binding_symbol(accessor_idx);
        let member_names = self.object_literal_member_names_for_accessor(accessor_idx);
        let receiver_aliases = self.object_literal_getter_receiver_aliases(body_idx, binding_sym);
        self.getter_self_reference_in_subtree(
            body_idx,
            name,
            binding_sym,
            &receiver_aliases,
            &member_names,
        )
    }

    /// Symbol of the variable an object-literal accessor's enclosing object
    /// literal initializes (`const o = { get x() {…} }` → the symbol of `o`),
    /// or `None` when the literal is not a direct variable initializer. Used to
    /// recognize `o.x` inside `o`'s own getter as a genuine self-reference,
    /// exactly as `this.x` is.
    fn object_literal_initializer_binding_symbol(
        &self,
        accessor_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let obj_idx = self.ctx.arena.get_extended(accessor_idx)?.parent;
        let obj_node = self.ctx.arena.get(obj_idx)?;
        if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let var_idx = self.ctx.arena.get_extended(obj_idx)?.parent;
        let var_node = self.ctx.arena.get(var_idx)?;
        if var_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(var_node)?;
        if var_decl.initializer != obj_idx {
            return None;
        }
        self.ctx.binder.get_node_symbol(var_decl.name)
    }

    fn object_literal_member_names_for_accessor(
        &mut self,
        accessor_idx: NodeIndex,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let Some(obj_idx) = self
            .ctx
            .arena
            .get_extended(accessor_idx)
            .map(|ext| ext.parent)
        else {
            return names;
        };
        let Some(obj_node) = self.ctx.arena.get(obj_idx) else {
            return names;
        };
        if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return names;
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(obj_node) else {
            return names;
        };
        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let name = if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                self.get_property_name_resolved(prop.name)
            } else if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                self.ctx
                    .arena
                    .get_method_decl(elem_node)
                    .and_then(|method| self.get_property_name_resolved(method.name))
            } else if elem_node.kind == syntax_kind_ext::GET_ACCESSOR
                || elem_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                self.ctx
                    .arena
                    .get_accessor(elem_node)
                    .and_then(|accessor| self.get_property_name_resolved(accessor.name))
            } else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                self.ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|shorthand| self.ctx.arena.get(shorthand.name))
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.to_string())
            } else {
                None
            };
            if let Some(name) = name {
                names.insert(name);
            }
        }
        names
    }

    fn object_literal_getter_receiver_aliases(
        &self,
        body_idx: NodeIndex,
        binding_sym: Option<SymbolId>,
    ) -> FxHashSet<SymbolId> {
        let mut aliases = FxHashSet::default();
        loop {
            let before = aliases.len();
            self.collect_object_literal_getter_receiver_aliases(
                body_idx,
                binding_sym,
                &mut aliases,
            );
            if aliases.len() == before {
                return aliases;
            }
        }
    }

    fn collect_object_literal_getter_receiver_aliases(
        &self,
        node_idx: NodeIndex,
        binding_sym: Option<SymbolId>,
        aliases: &mut FxHashSet<SymbolId>,
    ) {
        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => return,
            syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.ctx.arena.get_variable_declaration(node)
                    && decl.initializer.is_some()
                    && self.receiver_denotes_object_under_construction(
                        decl.initializer,
                        binding_sym,
                        aliases,
                    )
                    && let Some(alias_sym) = self.ctx.binder.get_node_symbol(decl.name)
                {
                    aliases.insert(alias_sym);
                }
            }
            _ => {}
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.collect_object_literal_getter_receiver_aliases(child_idx, binding_sym, aliases);
        }
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
            // A property access `obj.name` only references the parameter through
            // its object expression. The `.name` part is a member name resolved
            // in the object's namespace, never a reference to the sibling
            // parameter, so recursing into it would mis-flag `f(x, name = \`${x.name}\`)`
            // as a self-reference (TS2372/TS7022). Walk only the object side;
            // tsc resolves the property name to a member symbol, not the param.
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    self.collect_self_references_recursive(access.expression, name, refs);
                }
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

    /// Recursive helper for `object_literal_getter_has_self_reference`: does any
    /// `<receiver>.<name>` access in `node_idx`'s subtree have a receiver that
    /// denotes the object under construction? Recursion stops at nested
    /// function/class scopes, which rebind both `this` and the enclosing name.
    fn getter_self_reference_in_subtree(
        &self,
        node_idx: NodeIndex,
        name: &str,
        binding_sym: Option<SymbolId>,
        receiver_aliases: &FxHashSet<SymbolId>,
        object_member_names: &FxHashSet<String>,
    ) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && self.receiver_denotes_object_under_construction(
                access.expression,
                binding_sym,
                receiver_aliases,
            )
            && (ident.escaped_text == name
                || !object_member_names.contains(ident.escaped_text.as_str()))
        {
            return true;
        }

        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => {
                return false;
            }
            _ => {}
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.getter_self_reference_in_subtree(
                    child_idx,
                    name,
                    binding_sym,
                    receiver_aliases,
                    object_member_names,
                )
            })
    }

    /// Whether a property-access receiver expression evaluates to the object
    /// literal currently under construction. True for the synthetic `this`, an
    /// identifier resolving to the literal's own initializer binding, and any
    /// transparent wrapper (parens / `as` / `!` / `satisfies` / comma),
    /// array-literal index (`[this][0]`), element access, or conditional branch
    /// that flows one of those outward. False for an unrelated receiver
    /// (`ctx`, `mgr`, `g.sub`) or a call result (`foo(this)`), which `tsc`
    /// resolves to a different type whose `.<name>` member is not the accessor.
    fn receiver_denotes_object_under_construction(
        &self,
        receiver_idx: NodeIndex,
        binding_sym: Option<SymbolId>,
        receiver_aliases: &FxHashSet<SymbolId>,
    ) -> bool {
        let receiver_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions_and_comma(receiver_idx);
        let Some(node) = self.ctx.arena.get(receiver_idx) else {
            return false;
        };

        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }
        if node.kind == SyntaxKind::Identifier as u16 {
            let receiver_sym = self.resolve_identifier_symbol(receiver_idx);
            return binding_sym.is_some_and(|sym| receiver_sym == Some(sym))
                || receiver_sym.is_some_and(|sym| receiver_aliases.contains(&sym));
        }
        match node.kind {
            // `[this][0]`: the indexed result is one of the array's elements.
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.ctx.arena.get_literal_expr(node).is_some_and(|arr| {
                    arr.elements.nodes.iter().copied().any(|el| {
                        self.receiver_denotes_object_under_construction(
                            el,
                            binding_sym,
                            receiver_aliases,
                        )
                    })
                })
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.ctx.arena.get_access_expr(node).is_some_and(|access| {
                    self.receiver_denotes_object_under_construction(
                        access.expression,
                        binding_sym,
                        receiver_aliases,
                    )
                })
            }
            syntax_kind_ext::CONDITIONAL_EXPRESSION => self
                .ctx
                .arena
                .get_conditional_expr(node)
                .is_some_and(|cond| {
                    self.receiver_denotes_object_under_construction(
                        cond.when_true,
                        binding_sym,
                        receiver_aliases,
                    ) || self.receiver_denotes_object_under_construction(
                        cond.when_false,
                        binding_sym,
                        receiver_aliases,
                    )
                }),
            _ => false,
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
            refs.push((access.name_or_argument, ident.escaped_text.to_string()));
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
    ///
    /// Clears `preserve_literal_types` for the body walk so that the
    /// getter's own literal-widening decision is independent of the outer
    /// `return_expression_type` scope. When an object literal is itself a
    /// function's return expression, the outer scope sets the flag to
    /// preserve the obj literal's own literal property types, but the
    /// getter body must make its own widening decision (mirrors the
    /// function-expression branch in `return_expression_type`, where nested
    /// function-like inference does the same).
    pub(crate) fn infer_getter_return_type(&mut self, body_idx: NodeIndex) -> TypeId {
        self.infer_getter_return_type_for_node(tsz_parser::parser::NodeIndex::NONE, body_idx)
    }

    /// Infer a get-accessor's return type from its body, threading the accessor
    /// declaration node as the enclosing function.
    ///
    /// Passing the accessor node (rather than `NodeIndex::NONE`) gives the body
    /// walk the same enclosing-function context a method body receives in
    /// `call_signature_from_method` (circular-return tracking, self-reference
    /// detection), keeping getter and method return inference consistent.
    /// Callers that have no accessor node (e.g. object-literal getters, which
    /// establish their own context via `get_type_of_function`) pass
    /// `NodeIndex::NONE`.
    pub(crate) fn infer_getter_return_type_for_node(
        &mut self,
        accessor_idx: NodeIndex,
        body_idx: NodeIndex,
    ) -> TypeId {
        let prev = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = false;
        let r = self.infer_return_type_from_body(accessor_idx, body_idx, None);
        self.ctx.preserve_literal_types = prev;
        // tsc applies getWidenedType to an accessor's inferred type: in non-strict
        // mode `get x() { return null }` has type `any`, not `null`. Without this,
        // tsz keeps `null`/`undefined` and a call/member access on the accessor
        // value (`C.b()` where `b` is a getter returning null) reports a spurious
        // TS2349 (#94 accessor facet). Widening null/undefined -> any is permissive
        // and bidirectional with `any`, so it can only remove errors, never add one.
        if !self.ctx.strict_null_checks() {
            crate::query_boundaries::widening::widen_nullish_to_any_deep(self.ctx.types, r)
        } else {
            r
        }
    }

    /// Resolve a statement to the function declaration it contributes to
    /// statement-level function-implementation grouping, seeing through the
    /// parser's `EXPORT_DECLARATION` wrapper (`export function ...` /
    /// `export default function ...`). Returns the function node and whether
    /// the statement is a default export.
    pub(crate) fn statement_function_declaration_view(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(NodeIndex, bool)> {
        let node = self.ctx.arena.get(stmt_idx)?;
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            return Some((stmt_idx, false));
        }
        if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let export_decl = self.ctx.arena.get_export_decl(node)?;
            let clause = self.ctx.arena.get(export_decl.export_clause)?;
            if clause.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                return Some((export_decl.export_clause, export_decl.is_default_export));
            }
        }
        None
    }

    /// Check that all top-level function overload signatures have implementations.
    /// Reports errors 2389, 2391.
    ///
    /// The walk sees through export wrappers, so `export function` and
    /// `export default function` declarations join the same name-keyed
    /// grouping as bare declarations. Anonymous default-exported functions
    /// have no local name to group under; they are handled by the merged
    /// `default`-symbol pass (`check_default_export_function_group`), which
    /// also owns the cross-name TS2394 check tsc runs over the one `default`
    /// export symbol.
    pub(crate) fn check_function_implementations(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < statements.len() {
            let stmt_idx = statements[i];
            let Some((fn_idx, _)) = self.statement_function_declaration_view(stmt_idx) else {
                i += 1;
                continue;
            };

            if let Some(stmt_node) = self.ctx.arena.get(stmt_idx)
                && let Some(fn_node) = self.ctx.arena.get(fn_idx)
                && let Some(func) = self.ctx.arena.get_function(fn_node)
                && func.body.is_none()
            {
                // Suppress TS2391 when a parse error occurs within the statement span.
                // When `body.is_none()` and there are parse errors within the span,
                // the function was likely malformed (e.g. `function f() => 4;`).
                // This doesn't affect cases like `function f(a {` because the parser gives
                // those a body (`body_none=false`) so they never reach this path.
                if self.has_syntax_parse_errors() {
                    let fn_start = stmt_node.pos;
                    let fn_end = stmt_node.end;
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
                let is_declared = self.is_ambient_declaration(fn_idx);
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
                    if let Some(name) = self.get_function_name_from_node(fn_idx) {
                        let (has_impl, impl_name, impl_stmt_idx) =
                            self.find_function_impl(statements, i + 1, &name);
                        if has_impl
                            && impl_name.as_deref() == Some(name.as_str())
                            && let Some(impl_stmt_idx) = impl_stmt_idx
                            && !self.is_ambient_declaration(impl_stmt_idx)
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
                let func_name = self.get_function_name_from_node(fn_idx);
                if let Some(name) = func_name {
                    // Advance past consecutive bodyless overloads with the same name.
                    let mut last_overload_i = i;
                    let mut j = i + 1;
                    while j < statements.len() {
                        let Some((next_fn_idx, _)) =
                            self.statement_function_declaration_view(statements[j])
                        else {
                            break;
                        };
                        if let Some(next_node) = self.ctx.arena.get(next_fn_idx)
                            && let Some(next_func) = self.ctx.arena.get_function(next_node)
                            && next_func.body.is_none()
                        {
                            let next_name = self.get_function_name_from_node(next_fn_idx);
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
                        .statement_function_declaration_view(report_stmt_idx)
                        .and_then(|(f, _)| self.ctx.arena.get(f))
                        .and_then(|n| self.ctx.arena.get_function(n))
                        .map(|f| f.name)
                        .filter(|n| n.is_some())
                        .unwrap_or(report_stmt_idx);

                    let (has_impl, impl_name, impl_stmt_idx) =
                        self.find_function_impl(statements, last_overload_i + 1, &name);
                    if !has_impl {
                        self.error_at_node(
                                    report_error_node,
                                    "Function implementation is missing or not immediately following the declaration.",
                                    diagnostic_codes::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION
                                );
                    } else if let Some(impl_stmt_idx) = impl_stmt_idx {
                        if impl_name.as_deref() != Some(name.as_str()) {
                            // Wrong (or missing) implementation name — report at the
                            // implementation name. An anonymous default-exported
                            // implementation has no name node; tsc anchors at the
                            // whole declaration statement then.
                            let impl_error_node = self
                                .statement_function_declaration_view(impl_stmt_idx)
                                .and_then(|(f, _)| self.ctx.arena.get(f))
                                .and_then(|n| self.ctx.arena.get_function(n))
                                .map(|f| f.name)
                                .filter(|n| n.is_some())
                                .unwrap_or(impl_stmt_idx);
                            self.error_at_node(
                                impl_error_node,
                                &format!("Function implementation name must be '{name}'."),
                                diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                            );
                        } else {
                            let impl_is_declared = self.is_ambient_declaration(impl_stmt_idx);
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

        self.check_default_export_function_group(statements);
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

    /// Get the declaring class symbol for a private member.
    /// Returns the SymbolId of the class that contains the private member declaration.
    pub(crate) fn private_member_declaring_class_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
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

            // Return the symbol of the parent class
            return self.ctx.binder.get_node_symbol(ext.parent);
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

    pub(crate) fn get_explicit_this_type_annotation(
        &self,
        params: &[NodeIndex],
    ) -> Option<NodeIndex> {
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
                    if let Some(cached) = cached_instance_this
                        && cached != TypeId::ANY
                        && cached != TypeId::ERROR
                    {
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
            let object_property_count = |this: &Self, type_id| {
                crate::query_boundaries::common::object_shape_for_type(this.ctx.types, type_id)
                    .map(|shape| shape.properties.len())
                    .unwrap_or(0)
            };

            if let Some(cached) = cached_instance_this
                && cached != TypeId::ANY
                && cached != TypeId::ERROR
            {
                let cached_count = object_property_count(self, cached);
                let in_progress = self
                    .ctx
                    .class_instance_type_cache
                    .borrow()
                    .get(&class_idx)
                    .copied();
                if let Some(in_progress) = in_progress
                    && in_progress != TypeId::ANY
                    && in_progress != TypeId::ERROR
                    && object_property_count(self, in_progress) > cached_count
                {
                    if let Some(info) = self.ctx.enclosing_class.as_mut()
                        && info.class_idx == class_idx
                    {
                        info.cached_instance_this_type = Some(in_progress);
                    }
                    return Some(in_progress);
                }
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
    ///
    /// Must be called *after* `get_type_of_node_with_request` for the enclosing initializer, so
    /// that closures already processed with a contextual callable type are in
    /// `implicit_any_checked_closures` and can be skipped here.
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
            if let Some(func) = self.ctx.arena.get_function(node) {
                if !self.closure_has_contextual_type(node_idx) {
                    for (pi, &param_idx) in func.parameters.nodes.iter().enumerate() {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.maybe_report_implicit_any_parameter(param, false, pi);
                        }
                    }
                }
                // Always recurse into the body: deeply nested functions may not have
                // been contextually typed even when the outer closure was.
                if func.body.is_some() {
                    self.check_for_nested_function_ts7006(func.body);
                }
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
