//! Core implementation of the `Completions` provider.
//!
//! Contains constructor methods, scope-walking completion logic, member
//! completion dispatch, and symbol-detail rendering.

use std::borrow::Cow;

use rustc_hash::FxHashSet;

use super::*;

impl<'a> Completions<'a> {
    /// Create a new Completions provider.
    pub const fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
            interner: None,
            file_name: None,
            strict: false,
        }
    }

    /// Create a completions provider with type-aware member completion support.
    pub const fn new_with_types(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
            interner: Some(interner),
            file_name: Some(file_name),
            strict: false,
        }
    }

    /// Create a completions provider with type-aware member completion support and explicit strict mode.
    pub const fn with_strict(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
        strict: bool,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
            interner: Some(interner),
            file_name: Some(file_name),
            strict,
        }
    }

    /// Get completion suggestions at the given position.
    ///
    /// Returns a list of completion items for identifiers visible at the cursor position.
    /// Returns None if no completions are available.
    pub fn get_completions(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<CompletionItem>> {
        self.get_completions_internal(root, position, None, None, None)
    }

    /// Get completion suggestions at the given position with a persistent type cache.
    pub fn get_completions_with_cache(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<TypeCache>,
    ) -> Option<Vec<CompletionItem>> {
        self.get_completions_internal(root, position, Some(type_cache), None, None)
    }

    pub fn get_completions_with_caches(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<CompletionItem>> {
        self.get_completions_internal(
            root,
            position,
            Some(type_cache),
            Some(scope_cache),
            scope_stats,
        )
    }

    /// Get a full completion result including metadata like `is_new_identifier_location`.
    pub fn get_completion_result(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<CompletionResult> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = self.find_completions_node(root, offset);
        let member_target = self
            .member_completion_target(node_idx, offset)
            .or_else(|| self.marker_comment_member_completion_target(offset));
        let is_dotted_namespace = self.is_dotted_namespace_completion_context(offset);
        let is_member =
            !is_dotted_namespace && (member_target.is_some() || self.is_member_context(offset));
        let is_new_id = if is_member {
            false
        } else if is_dotted_namespace || self.should_offer_constructor_keyword(offset) {
            true
        } else {
            self.compute_is_new_identifier_location(root, offset)
        };
        let items = self.get_completions_internal(root, position, None, None, None)?;
        Some(CompletionResult {
            is_global_completion: !is_member,
            is_member_completion: is_member,
            is_new_identifier_location: is_new_id,
            default_commit_characters: (!is_new_id).then_some(vec![
                ".".to_string(),
                ",".to_string(),
                ";".to_string(),
            ]),
            entries: items,
        })
    }

    /// Collect inherited class members as completion candidates for class member snippets.
    pub fn get_class_member_snippet_candidates(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Vec<CompletionItem> {
        let Some(offset) = self.line_map.position_to_offset(position, self.source_text) else {
            return Vec::new();
        };
        let node_idx = self.find_completions_node(root, offset);
        let Some(class_idx) = self.find_enclosing_class_declaration(node_idx) else {
            return Vec::new();
        };
        let Some(base_expr) = self.class_extends_expression(class_idx) else {
            return Vec::new();
        };
        let mut candidates = self
            .get_member_completions(base_expr, None)
            .unwrap_or_default();
        if candidates.is_empty() {
            return candidates;
        }

        let declared_members = self.class_declared_member_names(class_idx);
        candidates.retain(|item| {
            (item.kind == CompletionItemKind::Method || item.kind == CompletionItemKind::Property)
                && !declared_members.contains(&item.label)
        });

        for item in &mut candidates {
            item.sort_text = Some(sort_priority::SUGGESTED_CLASS_MEMBERS.to_string());
        }

        candidates.sort_by(|a, b| a.label.cmp(&b.label));
        candidates
    }

    /// Check if the cursor is after a dot (member completion context).
    fn get_completions_internal(
        &self,
        root: NodeIndex,
        position: Position,
        mut type_cache: Option<&mut Option<TypeCache>>,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<CompletionItem>> {
        // 1. Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // 2. Find the node at this offset using improved lookup
        let node_idx = self.find_completions_node(root, offset);
        if self.is_dotted_namespace_completion_context(offset) {
            return Some(Vec::new());
        }

        // 3. Contextual string-literal completions for call arguments.
        // This path intentionally runs before no-completion suppression, because
        // ordinary string literals are suppressed by default.
        if self.interner.is_some()
            && self.file_name.is_some()
            && let Some(items) =
                self.get_string_literal_completions(node_idx, offset, type_cache.as_deref_mut())
        {
            return if items.is_empty() { None } else { Some(items) };
        }
        if self.interner.is_some()
            && self.file_name.is_some()
            && let Some(items) = self.get_contextual_string_literal_completions(
                node_idx,
                offset,
                type_cache.as_deref_mut(),
            )
        {
            return if items.is_empty() { None } else { Some(items) };
        }

        // 4a. Check for ".." (double dot) context — never complete after "..".
        // This must run before member target resolution because the parser may
        // create a PropertyAccessExpression for `q.` where the second `.` is the
        // cursor position.
        if self.is_after_double_dot(offset) {
            return None;
        }

        // 4b. Resolve member completion targets before lexical suppression checks.
        // Fourslash marker comments (e.g. `obj./**/`) often place the cursor inside
        // comment trivia where no-completion filters would otherwise short-circuit.
        let member_target = self
            .member_completion_target(node_idx, offset)
            .or_else(|| self.marker_comment_member_completion_target(offset));
        if let Some(expr_idx) = member_target {
            if let Some(items) = self.get_member_completions(expr_idx, type_cache.as_deref_mut())
                && !items.is_empty()
            {
                return Some(items);
            }
            // If member completions returned empty for `this.`, don't fall
            // through to global completions — `this` in a non-class context
            // should have no completions.
            if self
                .arena
                .get(expr_idx)
                .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16)
            {
                return None;
            }
        }
        if let Some(items) = self.get_typeof_query_parameter_completions(node_idx, offset)
            && !items.is_empty()
        {
            return Some(items);
        }
        if let Some(items) = self.get_meta_property_completions(offset) {
            return Some(items);
        }
        let is_orphan_dot = member_target.is_none() && self.is_member_context(offset);
        // If cursor follows a dot but there's no valid expression before it
        // (e.g., source is just "."), return no completions.
        if is_orphan_dot {
            return None;
        }
        // (Double-dot check already done at step 4a above.)
        let member_request = member_target.is_some() || self.is_member_context(offset);
        let global_this_member_fallback = member_target
            .and_then(|idx| self.arena.get_identifier_text(idx))
            .is_some_and(|name| name == "globalThis");

        // 5. Filter out positions where completions should not appear
        if self.is_in_no_completion_context(offset) {
            if self.is_string_property_name_completion_context(node_idx)
                && self.interner.is_some()
                && self.file_name.is_some()
                && let Some(items) =
                    self.get_object_literal_completions(node_idx, offset, type_cache)
            {
                return if items.is_empty() { None } else { Some(items) };
            }
            return Some(Vec::new());
        }

        // 5b. Class/interface body member position: return member keywords
        if !member_request && let Some(context) = self.member_body_context(node_idx, offset) {
            return Some(self.get_member_keyword_completions(context));
        }

        // 6. Check for object literal property completion (contextual completions)
        if self.interner.is_some()
            && self.file_name.is_some()
            && let Some(items) = self.get_object_literal_completions(node_idx, offset, type_cache)
        {
            return if items.is_empty() { None } else { Some(items) };
        }

        // 7. Get the scope chain at this position
        let mut completions = Vec::new();
        let mut seen_names = FxHashSet::default();

        if !global_this_member_fallback {
            let mut walker = ScopeWalker::new(self.arena, self.binder);
            let scope_chain = if let Some(scope_cache) = scope_cache {
                Cow::Borrowed(walker.get_scope_chain_cached(
                    root,
                    node_idx,
                    scope_cache,
                    scope_stats,
                ))
            } else {
                Cow::Owned(walker.get_scope_chain(root, node_idx))
            };

            // 8. Collect all visible identifiers from the scope chain
            // Walk scopes from innermost to outermost
            for scope in scope_chain.iter().rev() {
                for (name, symbol_id) in scope.iter() {
                    if seen_names.contains(name) {
                        continue;
                    }

                    if let Some(symbol) = self.binder.symbols.get(*symbol_id) {
                        // Synthetic CommonJS helpers should not appear in globals-style completion lists.
                        // Keep user-declared symbols with these names by requiring no declarations.
                        if matches!(
                            name.as_str(),
                            "exports" | "require" | "module" | "__dirname" | "__filename"
                        ) && symbol.declarations.is_empty()
                            && symbol.value_declaration.is_none()
                        {
                            continue;
                        }
                        if self
                            .parameter_declaration_node(symbol)
                            .is_some_and(|param_decl| {
                                !self.parameter_symbol_visible_at_offset(param_decl, offset)
                            })
                        {
                            continue;
                        }

                        seen_names.insert(name.clone());
                        let mut kind = self.determine_completion_kind(symbol);
                        if kind == CompletionItemKind::Variable && self.symbol_is_parameter(symbol)
                        {
                            kind = CompletionItemKind::Parameter;
                        }
                        let mut item = CompletionItem::new(name.clone(), kind);
                        item.sort_text = Some(default_sort_text(kind).to_string());

                        if kind == CompletionItemKind::Parameter
                            && let Some(param_type) = self.parameter_annotation_text(symbol)
                        {
                            item = item.with_detail(param_type);
                        } else if matches!(
                            kind,
                            CompletionItemKind::Const
                                | CompletionItemKind::Let
                                | CompletionItemKind::Variable
                        ) && let Some(detail) =
                            self.variable_completion_type_detail(symbol, kind)
                        {
                            item = item.with_detail(detail);
                        } else if let Some(detail) = self.get_symbol_detail(symbol) {
                            item = item.with_detail(detail);
                        }
                        if let Some(modifiers) = self.build_kind_modifiers(symbol) {
                            item.kind_modifiers = Some(modifiers);
                        }
                        if kind == CompletionItemKind::Function
                            || kind == CompletionItemKind::Method
                        {
                            item.insert_text = Some(format!("{name}($1)"));
                            item.is_snippet = true;
                        }

                        let decl_node = if symbol.value_declaration.is_some() {
                            symbol.value_declaration
                        } else {
                            symbol
                                .declarations
                                .first()
                                .copied()
                                .unwrap_or(NodeIndex::NONE)
                        };
                        if decl_node.is_some() {
                            let doc = jsdoc_for_node(self.arena, root, decl_node, self.source_text);
                            if !doc.is_empty() {
                                item = item.with_documentation(doc);
                            }
                        }

                        completions.push(item);
                    }
                }
            }
        } else {
            let _ = (scope_cache, scope_stats);
        }

        // 9. Add global variables (globalThis, Array, etc.)
        //    These are always available and match fourslash globalsVars order.
        let inside_func = if global_this_member_fallback {
            false
        } else {
            self.is_inside_function(offset)
        };
        if !seen_names.contains("globalThis") {
            seen_names.insert("globalThis".to_string());
            let mut item =
                CompletionItem::new("globalThis".to_string(), CompletionItemKind::Module);
            item.sort_text = Some(
                if member_request {
                    sort_priority::LOCATION_PRIORITY
                } else {
                    sort_priority::GLOBALS_OR_KEYWORDS
                }
                .to_string(),
            );
            completions.push(item);
        }

        for &(name, kind) in GLOBAL_VARS {
            if !seen_names.contains(name) {
                let name_str = name.to_string();
                seen_names.insert(name_str.clone());
                let mut item = CompletionItem::new(name_str, kind);
                let is_deprecated = DEPRECATED_GLOBALS.contains(&name);
                if is_deprecated {
                    item.sort_text = Some(sort_priority::deprecated(if member_request {
                        sort_priority::LOCATION_PRIORITY
                    } else {
                        sort_priority::GLOBALS_OR_KEYWORDS
                    }));
                    item.kind_modifiers = Some("deprecated,declare".to_string());
                } else {
                    item.sort_text = Some(
                        if member_request {
                            sort_priority::LOCATION_PRIORITY
                        } else {
                            sort_priority::GLOBALS_OR_KEYWORDS
                        }
                        .to_string(),
                    );
                    item.kind_modifiers = Some("declare".to_string());
                }
                if kind == CompletionItemKind::Function {
                    item.insert_text = Some(format!("{name}($1)"));
                    item.is_snippet = true;
                }
                completions.push(item);
            }
        }

        if !seen_names.contains("undefined") {
            seen_names.insert("undefined".to_string());
            let mut item =
                CompletionItem::new("undefined".to_string(), CompletionItemKind::Variable);
            item.sort_text = Some(
                if member_request {
                    sort_priority::LOCATION_PRIORITY
                } else {
                    sort_priority::GLOBALS_OR_KEYWORDS
                }
                .to_string(),
            );
            completions.push(item);
        }

        // 10. If inside a function, also add "arguments" as a built-in variable
        if inside_func && !seen_names.contains("arguments") {
            seen_names.insert("arguments".to_string());
            let mut item =
                CompletionItem::new("arguments".to_string(), CompletionItemKind::Variable);
            item.sort_text = Some(sort_priority::LOCATION_PRIORITY.to_string());
            completions.push(item);
        }

        if !member_request
            && !seen_names.contains("constructor")
            && self.should_offer_constructor_keyword(offset)
        {
            seen_names.insert("constructor".to_string());
            let mut ctor =
                CompletionItem::new("constructor".to_string(), CompletionItemKind::Keyword);
            ctor.sort_text = Some(sort_priority::GLOBALS_OR_KEYWORDS.to_string());
            completions.push(ctor);
        }

        // 11. Add keywords for non-member completions
        if !member_request {
            let keywords = if inside_func {
                KEYWORDS_INSIDE_FUNCTION
            } else {
                KEYWORDS
            };
            for kw in keywords.iter().copied() {
                if !seen_names.contains(kw) {
                    let mut kw_item =
                        CompletionItem::new(kw.to_string(), CompletionItemKind::Keyword);
                    kw_item.sort_text = Some(sort_priority::KEYWORD.to_string());
                    completions.push(kw_item);
                }
            }
        }

        if completions.is_empty() {
            None
        } else {
            completions.sort_by(|a, b| {
                let sa = a.effective_sort_text();
                let sb = b.effective_sort_text();
                compare_case_sensitive_ui(sa, sb)
                    .then_with(|| compare_case_sensitive_ui(&a.label, &b.label))
            });
            Some(completions)
        }
    }

    /// Determine the completion kind from a symbol.
    pub(super) fn determine_completion_kind(
        &self,
        symbol: &tsz_binder::Symbol,
    ) -> CompletionItemKind {
        use tsz_binder::symbol_flags;

        if symbol.flags & symbol_flags::ALIAS != 0 {
            CompletionItemKind::Alias
        } else if symbol.flags & symbol_flags::CONSTRUCTOR != 0 {
            CompletionItemKind::Constructor
        } else if symbol.flags & symbol_flags::FUNCTION != 0 {
            CompletionItemKind::Function
        } else if symbol.flags & symbol_flags::CLASS != 0 {
            CompletionItemKind::Class
        } else if symbol.flags & symbol_flags::INTERFACE != 0 {
            CompletionItemKind::Interface
        } else if symbol.flags & symbol_flags::REGULAR_ENUM != 0
            || symbol.flags & symbol_flags::CONST_ENUM != 0
        {
            CompletionItemKind::Enum
        } else if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            CompletionItemKind::TypeAlias
        } else if symbol.flags & symbol_flags::TYPE_PARAMETER != 0 {
            CompletionItemKind::TypeParameter
        } else if symbol.flags & symbol_flags::METHOD != 0 {
            CompletionItemKind::Method
        } else if symbol.flags & symbol_flags::PROPERTY != 0 {
            CompletionItemKind::Property
        } else if symbol.flags & symbol_flags::VALUE_MODULE != 0
            || symbol.flags & symbol_flags::NAMESPACE_MODULE != 0
        {
            CompletionItemKind::Module
        } else if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            // Distinguish const from let by checking the declaration node flags
            if self.is_const_declaration(symbol) {
                CompletionItemKind::Const
            } else {
                CompletionItemKind::Let
            }
        } else {
            // Default to variable for var and parameters
            CompletionItemKind::Variable
        }
    }

    /// Check if a block-scoped variable symbol was declared with `const`.
    fn is_const_declaration(&self, symbol: &tsz_binder::Symbol) -> bool {
        use tsz_parser::parser::flags::node_flags;

        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            return false;
        };

        // Walk up to find the VariableDeclarationList parent
        let mut current = decl;
        for _ in 0..3 {
            if let Some(ext) = self.arena.get_extended(current) {
                current = ext.parent;
                if let Some(node) = self.arena.get(current)
                    && node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                {
                    return (node.flags as u32) & node_flags::CONST != 0;
                }
            } else {
                break;
            }
        }
        false
    }

    fn symbol_is_parameter(&self, symbol: &tsz_binder::Symbol) -> bool {
        self.parameter_declaration_node(symbol).is_some()
    }

    fn parameter_declaration_node(&self, symbol: &tsz_binder::Symbol) -> Option<NodeIndex> {
        if symbol.value_declaration.is_some() {
            if self
                .arena
                .get(symbol.value_declaration)
                .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
            {
                return Some(symbol.value_declaration);
            }
            if let Some(ext) = self.arena.get_extended(symbol.value_declaration)
                && self
                    .arena
                    .get(ext.parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
            {
                return Some(ext.parent);
            }
        }
        for decl in symbol.declarations.iter().copied() {
            if self
                .arena
                .get(decl)
                .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
            {
                return Some(decl);
            }
            if let Some(ext) = self.arena.get_extended(decl)
                && self
                    .arena
                    .get(ext.parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::PARAMETER)
            {
                return Some(ext.parent);
            }
        }
        None
    }

    fn parameter_annotation_text(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.arena.get(decl)?;
        if node.kind != syntax_kind_ext::PARAMETER {
            return None;
        }
        let param = self.arena.get_parameter(node)?;
        if !param.type_annotation.is_some() {
            return None;
        }
        let type_node = self.arena.get(param.type_annotation)?;
        let start = type_node.pos as usize;
        let end = type_node.end.min(self.source_text.len() as u32) as usize;
        (start < end).then(|| {
            let mut text = self.source_text[start..end].trim().to_string();
            while text.ends_with(',') || text.ends_with(';') {
                text.pop();
                text = text.trim_end().to_string();
            }
            while text.ends_with(')') {
                let opens = text.chars().filter(|&c| c == '(').count();
                let closes = text.chars().filter(|&c| c == ')').count();
                if closes > opens {
                    text.pop();
                    text = text.trim_end().to_string();
                } else {
                    break;
                }
            }
            text
        })
    }

    fn parameter_symbol_visible_at_offset(&self, param_decl: NodeIndex, offset: u32) -> bool {
        let mut current = param_decl;
        for _ in 0..16 {
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                if let Some(function) = self.arena.get_function(node)
                    && function.body.is_some()
                    && let Some(body_node) = self.arena.get(function.body)
                {
                    return offset <= body_node.end;
                }
                return offset <= node.end;
            }
        }
        true
    }

    /// Check if the cursor is at a class/interface member declaration position
    /// (after `{`, `;`, or `}` inside a class or interface body).
    /// Returns Some("class") or Some("interface") if at a member position.
    fn member_body_context(&self, node_idx: NodeIndex, offset: u32) -> Option<&'static str> {
        // Use AST-based check and verify the offset is strictly inside
        // the class/interface body braces (not after the closing brace).
        let in_class = self.is_in_class_body_context(node_idx)
            && self.offset_is_inside_body_braces(node_idx, offset);
        let in_interface = !in_class
            && self.is_in_interface_body_context(node_idx)
            && self.offset_is_inside_body_braces(node_idx, offset);
        if !in_class && !in_interface {
            return None;
        }
        let bytes = self.source_text.as_bytes();
        let idx = offset as usize;
        let mut j = idx;
        while j > 0 && bytes[j - 1].is_ascii_whitespace() {
            j -= 1;
        }
        // Check for member position: after `{`, `;`, `}`, or after
        // a JSDoc comment (`*/`)
        if j == 0
            || matches!(bytes[j - 1], b'{' | b';' | b'}')
            || (j >= 2 && bytes[j - 2] == b'*' && bytes[j - 1] == b'/')
        {
            return Some(if in_class { "class" } else { "interface" });
        }
        None
    }

    /// Verify that the byte offset is strictly between the `{` and `}` of
    /// the nearest class or interface declaration. Prevents false positives
    /// when the cursor is after the closing brace.
    fn offset_is_inside_body_braces(&self, node_idx: NodeIndex, offset: u32) -> bool {
        let mut current = node_idx;
        for _ in 0..15 {
            if let Some(node) = self.arena.get(current)
                && (node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    || node.kind == syntax_kind_ext::INTERFACE_DECLARATION)
            {
                let start = node.pos as usize;
                let end = node.end as usize;
                if end <= self.source_text.len()
                    && let Some(open) = self.source_text[start..end].find('{')
                {
                    let open_offset = start + open;
                    return (offset as usize) > open_offset && (offset as usize) < end;
                }
                return false;
            }
            if let Some(ext) = self.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }
        true
    }

    /// Check if node is inside an interface body.
    fn is_in_interface_body_context(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        for _ in 0..15 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    return true;
                }
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::SOURCE_FILE
                {
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }
        false
    }

    /// Return keyword completions for class or interface member positions.
    fn get_member_keyword_completions(&self, context: &str) -> Vec<CompletionItem> {
        let keywords: &[&str] = if context == "interface" {
            &["readonly"]
        } else {
            &[
                "abstract",
                "accessor",
                "async",
                "constructor",
                "declare",
                "get",
                "override",
                "private",
                "protected",
                "public",
                "readonly",
                "set",
                "static",
            ]
        };
        keywords
            .iter()
            .map(|kw| {
                let mut item = CompletionItem::new(kw.to_string(), CompletionItemKind::Keyword);
                item.sort_text = Some(sort_priority::GLOBALS_OR_KEYWORDS.to_string());
                item
            })
            .collect()
    }

    fn variable_completion_type_detail(
        &self,
        symbol: &tsz_binder::Symbol,
        kind: CompletionItemKind,
    ) -> Option<String> {
        let decl = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.arena.get(decl)?;
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(node)?;

        if var_decl.type_annotation.is_some() {
            let type_node = self.arena.get(var_decl.type_annotation)?;
            let start = type_node.pos as usize;
            let end = type_node.end.min(self.source_text.len() as u32) as usize;
            if start < end {
                return Some(self.source_text[start..end].trim().to_string());
            }
        }

        if kind != CompletionItemKind::Const || !var_decl.initializer.is_some() {
            return None;
        }
        let init_node = self.arena.get(var_decl.initializer)?;
        let start = init_node.pos as usize;
        let end = init_node.end.min(self.source_text.len() as u32) as usize;
        if start >= end {
            return None;
        }
        let init_text = self.source_text[start..end].trim();
        let bytes = init_text.as_bytes();
        if bytes.len() >= 2
            && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
        {
            return Some(init_text.to_string());
        }
        if init_text == "true" || init_text == "false" {
            return Some(init_text.to_string());
        }
        if init_text
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '_' || ch == '.' || ch == '-' || ch == '+')
            && init_text.chars().any(|ch| ch.is_ascii_digit())
        {
            return Some(init_text.to_string());
        }
        None
    }

    fn get_meta_property_completions(&self, offset: u32) -> Option<Vec<CompletionItem>> {
        let end = (offset as usize).min(self.source_text.len());
        let prefix = Self::strip_trailing_fourslash_marker(&self.source_text[..end]).trim_end();
        let before_dot = prefix.strip_suffix('.')?;
        let expr = before_dot.trim_end();
        if expr.ends_with("import.meta") {
            if let Some(target) = self.import_meta_member_target(offset)
                && let Some(items) = self.get_member_completions(target, None)
                && !items.is_empty()
            {
                return Some(items);
            }
            return Some(self.get_import_meta_member_completions());
        }
        let token_start = expr
            .rfind(|c: char| !(c == '_' || c == '$' || c.is_ascii_alphanumeric()))
            .map_or(0, |idx| idx + 1);
        let token = &expr[token_start..];
        let before_token = &expr[..token_start];
        let has_ident_before = before_token
            .chars()
            .next_back()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric());
        if has_ident_before {
            return None;
        }

        if token == "import" {
            let mut item = CompletionItem::new("meta".to_string(), CompletionItemKind::Property);
            item.sort_text = Some(sort_priority::MEMBER.to_string());
            item = item.with_detail("ImportMeta".to_string());
            return Some(vec![item]);
        }
        if token == "new" {
            if self.is_inside_function(offset) {
                let mut item =
                    CompletionItem::new("target".to_string(), CompletionItemKind::Property);
                item.sort_text = Some(sort_priority::MEMBER.to_string());
                item = item.with_detail("() => void".to_string());
                return Some(vec![item]);
            }
            return Some(Vec::new());
        }
        None
    }

    fn get_typeof_query_parameter_completions(
        &self,
        node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<CompletionItem>> {
        let end = (offset as usize).min(self.source_text.len());
        let prefix = Self::strip_trailing_fourslash_marker(&self.source_text[..end]).trim_end();
        if !prefix.ends_with("typeof") {
            return None;
        }

        let mut current = node_idx;
        let mut depth = 0;
        while current.is_some() && depth < 32 {
            let node = self.arena.get(current)?;
            if (node.kind == syntax_kind_ext::FUNCTION_TYPE
                || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE)
                && let Some(function_type) = self.arena.get_function_type(node)
            {
                let mut items = Vec::new();
                let mut seen = FxHashSet::default();
                for &param_idx in &function_type.parameters.nodes {
                    let Some(param_node) = self.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.arena.get_parameter(param_node) else {
                        continue;
                    };
                    let Some(name) = self.arena.get_identifier_text(param.name) else {
                        continue;
                    };
                    let name_str = name.to_string();
                    if !seen.insert(name_str.clone()) {
                        continue;
                    }
                    let mut item =
                        CompletionItem::new(name_str, CompletionItemKind::Parameter);
                    item.sort_text = Some(sort_priority::LOCATION_PRIORITY.to_string());
                    items.push(item);
                }
                items.sort_by(|a, b| a.label.cmp(&b.label));
                return Some(items);
            }
            let ext = self.arena.get_extended(current)?;
            if ext.parent == current {
                break;
            }
            current = ext.parent;
            depth += 1;
        }
        None
    }

    fn get_import_meta_member_completions(&self) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let mut seen = FxHashSet::default();
        for decl_node in &self.arena.nodes {
            if decl_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                continue;
            }
            let Some(iface) = self.arena.get_interface(decl_node) else {
                continue;
            };
            if self.arena.get_identifier_text(iface.name) != Some("ImportMeta") {
                continue;
            }
            for &member_idx in &iface.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE
                    && member_node.kind != syntax_kind_ext::METHOD_SIGNATURE
                {
                    continue;
                }
                let Some(signature) = self.arena.get_signature(member_node) else {
                    continue;
                };
                let Some(name) = self.arena.get_identifier_text(signature.name) else {
                    continue;
                };
                if !seen.insert(name.to_string()) {
                    continue;
                }
                let kind = if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE {
                    CompletionItemKind::Method
                } else {
                    CompletionItemKind::Property
                };
                let mut item = CompletionItem::new(name.to_string(), kind);
                item.sort_text = Some(sort_priority::MEMBER.to_string());
                items.push(item);
            }
        }
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items
    }

    fn import_meta_member_target(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset.saturating_sub(1));
        for _ in 0..20 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(node)
                && self.arena.get_identifier_text(access.name_or_argument) == Some("meta")
            {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            if ext.parent == current {
                break;
            }
            current = ext.parent;
        }
        None
    }

    /// Get detail information for a symbol (e.g., "const", "function", "class").
    pub(super) fn member_completion_target(
        &self,
        node_idx: NodeIndex,
        offset: u32,
    ) -> Option<NodeIndex> {
        let mut current = node_idx;

        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(node)?;
                let expr_node = self.arena.get(access.expression)?;
                if offset >= expr_node.end && offset <= node.end {
                    return Some(access.expression);
                }
            }
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qualified = self.arena.get_qualified_name(node)?;
                let left_node = self.arena.get(qualified.left)?;
                if offset >= left_node.end && offset <= node.end {
                    return Some(qualified.left);
                }
            }

            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        None
    }

    fn marker_comment_member_completion_target(&self, offset: u32) -> Option<NodeIndex> {
        let bytes = self.source_text.as_bytes();
        let len = bytes.len() as u32;
        if len == 0 {
            return None;
        }
        let mut cursor = offset.min(len);

        loop {
            // If cursor is inside a block comment (`/*...*/`), jump back to the
            // marker start so member-context detection can inspect the preceding
            // `.`/`?.` token.
            let scan_end = cursor as usize;
            let prefix = &self.source_text[..scan_end];
            if let Some(block_start) = prefix.rfind("/*") {
                let block_body = &self.source_text[block_start + 2..scan_end];
                if !block_body.contains("*/") {
                    cursor = block_start as u32;
                    continue;
                }
            }

            while cursor > 0 && bytes[(cursor - 1) as usize].is_ascii_whitespace() {
                cursor -= 1;
            }

            if cursor >= 2
                && bytes[(cursor - 2) as usize] == b'*'
                && bytes[(cursor - 1) as usize] == b'/'
            {
                cursor -= 2;
                while cursor >= 2 {
                    if bytes[(cursor - 2) as usize] == b'/' && bytes[(cursor - 1) as usize] == b'*'
                    {
                        cursor -= 2;
                        break;
                    }
                    cursor -= 1;
                }
                continue;
            }

            break;
        }

        let line_start = self.source_text[..cursor as usize]
            .rfind('\n')
            .map_or(0, |idx| idx + 1);
        let line_prefix = &self.source_text[line_start..cursor as usize];
        if line_prefix.contains("//") {
            let comment_pos = line_prefix.find("//").unwrap_or(usize::MAX);
            if !(comment_pos == 0 && line_prefix.starts_with("////")) {
                return None;
            }
        }

        if cursor == 0 {
            return None;
        }
        // Check for `.` or `?.` (optional chaining)
        if bytes[(cursor - 1) as usize] != b'.' {
            return None;
        }

        let dot = cursor - 1;
        // Skip the `?` in `?.` (optional chaining)
        let scan_from = if dot > 0 && bytes[(dot - 1) as usize] == b'?' {
            dot - 1
        } else {
            dot
        };
        if scan_from > 0 {
            let node_idx = find_node_at_offset(self.arena, scan_from - 1);
            if node_idx.is_some()
                && let Some(node) = self.arena.get(node_idx)
                && node.kind == SyntaxKind::RegularExpressionLiteral as u16
            {
                return Some(node_idx);
            }
        }
        let mut ident_end = scan_from;
        while ident_end > 0 && bytes[(ident_end - 1) as usize].is_ascii_whitespace() {
            ident_end -= 1;
        }
        let mut ident_start = ident_end;
        while ident_start > 0 {
            let ch = bytes[(ident_start - 1) as usize];
            if ch == b'_' || ch == b'$' || ch.is_ascii_alphanumeric() {
                ident_start -= 1;
            } else {
                break;
            }
        }
        if ident_start >= ident_end {
            return None;
        }

        let mut current = find_node_at_offset(self.arena, ident_end.saturating_sub(1));
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16
                && node.pos <= ident_start
                && node.end >= ident_end
            {
                if let Some(ext) = self.arena.get_extended(current)
                    && let Some(parent) = self.arena.get(ext.parent)
                    && parent.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.arena.get_access_expr(parent)
                    && access.name_or_argument == current
                {
                    return Some(ext.parent);
                }
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        let mut current = find_node_at_offset(self.arena, ident_end.saturating_sub(1));
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION && node.end == ident_end {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        None
    }
}
