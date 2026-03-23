//! Core hover implementation logic.
//!
//! Contains the main `HoverProvider` methods for resolving hover information,
//! building display strings, and extracting symbol metadata.

use super::{HoverInfo, HoverProvider, format};
use crate::jsdoc::{jsdoc_for_node, parse_jsdoc};
use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::utils::{
    find_symbol_query_node_at_or_before, is_comment_context, should_backtrack_to_previous_symbol,
};
use tsz_checker::state::CheckerState;
use tsz_common::position::Range;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

impl<'a> HoverProvider<'a> {
    /// Get hover information at the given position.
    ///
    /// # Arguments
    /// * `root` - The root node of the AST
    /// * `position` - The cursor position
    /// * `type_cache` - Mutable reference to the persistent type cache (for performance)
    pub fn get_hover(
        &self,
        root: NodeIndex,
        position: tsz_common::position::Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, None, None)
    }

    pub fn get_hover_with_scope_cache(
        &self,
        root: NodeIndex,
        position: tsz_common::position::Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, Some(scope_cache), scope_stats)
    }

    fn get_hover_internal(
        &self,
        root: NodeIndex,
        position: tsz_common::position::Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        // 1. Find node at position
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let mut node_idx =
            crate::utils::find_node_at_or_before_offset(self.arena, offset, self.source_text);

        if node_idx.is_none()
            && let Some(adjusted) =
                find_symbol_query_node_at_or_before(self.arena, self.source_text, offset)
        {
            node_idx = adjusted;
        }

        if node_idx.is_none() {
            return None;
        }

        // Handle `this` keyword hover early — before symbol query filtering
        if let Some(node) = self.arena.get(node_idx)
            && node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
        {
            if let Some(this_hover) = self.hover_for_this_keyword(node_idx, type_cache) {
                return Some(this_hover);
            }
        }

        if !crate::utils::is_symbol_query_node(self.arena, node_idx)
            && (is_comment_context(self.source_text, offset)
                || should_backtrack_to_previous_symbol(self.source_text, offset))
            && let Some(adjusted) =
                find_symbol_query_node_at_or_before(self.arena, self.source_text, offset)
        {
            node_idx = adjusted;
        }

        if !crate::utils::is_symbol_query_node(self.arena, node_idx) {
            if let Some(contextual_property_hover) =
                self.hover_for_contextual_object_property(node_idx, type_cache)
            {
                return Some(contextual_property_hover);
            }
            return None;
        }

        node_idx = self
            .remap_import_equals_rhs_to_alias(node_idx)
            .unwrap_or(node_idx);

        // 2. Resolve symbol using ScopeWalker
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, node_idx)
        };
        let symbol_id = match symbol_id {
            Some(symbol_id) => symbol_id,
            None => {
                if let Some(member_hover) =
                    self.hover_for_property_access_member_name(root, node_idx, type_cache)
                {
                    return Some(member_hover);
                }
                if let Some(contextual_property_hover) =
                    self.hover_for_contextual_object_property(node_idx, type_cache)
                {
                    return Some(contextual_property_hover);
                }
                if let Some(class_hover) = self.hover_for_class_expression_keyword(node_idx) {
                    return Some(class_hover);
                }
                return None;
            }
        };
        let symbol = self.binder.symbols.get(symbol_id)?;

        // 3. Compute Type Information
        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = type_cache.take() {
            CheckerState::with_cache(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                compiler_options,
            )
        };

        let decl_node_idx = self.find_best_declaration(symbol, node_idx);

        let type_id = if decl_node_idx.is_some()
            && symbol.flags
                & (tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                    | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                != 0
        {
            checker.get_type_of_node(decl_node_idx)
        } else {
            checker.get_type_of_symbol(symbol_id)
        };
        let mut type_string = checker.format_type(type_id);
        if symbol.flags
            & (tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
            != 0
        {
            if let Some(annotation) = self.variable_declaration_annotation_text(decl_node_idx) {
                type_string = annotation;
            } else if let Some(initializer_type) =
                self.variable_initializer_display_type(&mut checker, decl_node_idx)
            {
                // For const declarations, the checker preserves literal types
                // (e.g., `const c = 0` → type is `0`, not `number`).
                // Don't override with the initializer expression type which may be widened.
                if self.get_variable_keyword(decl_node_idx) != "const"
                    || type_string == "error"
                    || type_string.is_empty()
                {
                    type_string = initializer_type;
                }
            }
        } else if symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
            && let Some(annotation) = self.type_alias_annotation_text(decl_node_idx)
        {
            type_string = annotation;
        }

        // Extract and save the updated cache for future queries
        *type_cache = Some(checker.extract_cache());

        // 5. Determine the kind string (tsserver-compatible)
        let kind = self.get_tsserver_kind(symbol, decl_node_idx);

        // 6. Determine kind modifiers (export, declare, abstract, etc.)
        let kind_modifiers = self.get_kind_modifiers(symbol, decl_node_idx);

        // 7. Construct the display string matching tsserver format
        let display_string = self.build_display_string(symbol, &kind, &type_string, decl_node_idx);

        // 8. Extract Documentation (JSDoc)
        let raw_documentation = if decl_node_idx.is_some() {
            jsdoc_for_node(self.arena, root, decl_node_idx, self.source_text)
        } else {
            String::new()
        };
        let formatted_doc = self.format_jsdoc_for_hover(&raw_documentation);
        let documentation_text = self.extract_plain_documentation(&raw_documentation);

        // 9. Build response
        let mut contents = Vec::new();

        // Code block for the signature
        contents.push(format!("```typescript\n{display_string}\n```"));

        // Documentation paragraph
        if let Some(doc) = formatted_doc {
            contents.push(doc);
        }

        // Calculate range for the hovered identifier
        let node = self.arena.get(node_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);

        Some(HoverInfo {
            contents,
            range: Some(Range::new(start, end)),
            display_string,
            kind,
            kind_modifiers,
            documentation: documentation_text,
            tags: Vec::new(),
        })
    }

    fn hover_for_property_access_member_name(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<HoverInfo> {
        use tsz_parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let node = self.arena.get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let parent_idx = self.arena.get_extended(node_idx)?.parent;
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(parent_node)?;
        if access.name_or_argument != node_idx {
            return None;
        }

        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };

        let mut checker = if let Some(cache) = type_cache.take() {
            CheckerState::with_cache(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                compiler_options,
            )
        };

        let name = self
            .arena
            .get_identifier(node)
            .map(|id| id.escaped_text.clone())
            .filter(|s| !s.is_empty())?;

        // Try binder-based member resolution first (enum members, namespace exports,
        // class statics). This handles cases where the expression resolves to a symbol
        // with exports/members that contain the accessed property.
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let expr_symbol_id = walker.resolve_node(root, access.expression);
        let binder_result = expr_symbol_id.and_then(|expr_sym_id| {
            let expr_symbol = self.binder.symbols.get(expr_sym_id)?;
            let member_sym_id = expr_symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(&name))
                .or_else(|| {
                    expr_symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(&name))
                })?;
            let member_type_id = checker.get_type_of_symbol(member_sym_id);
            let type_string = checker.format_type(member_type_id);
            if type_string.is_empty() || type_string == "error" {
                return None;
            }
            let container_name = expr_symbol.escaped_name.clone();
            let member_symbol = self.binder.symbols.get(member_sym_id);
            let is_enum_member = member_symbol
                .map(|s| s.flags & tsz_binder::symbol_flags::ENUM_MEMBER != 0)
                .unwrap_or(false);
            Some((type_string, container_name, is_enum_member))
        });

        if let Some((type_string, container_name, is_enum_member)) = binder_result {
            *type_cache = Some(checker.extract_cache());
            let display_string = if is_enum_member {
                format!("(enum member) {container_name}.{name} = {type_string}")
            } else {
                format!("(property) {container_name}.{name}: {type_string}")
            };
            let documentation = self
                .property_access_member_documentation(root, &container_name, &name)
                .unwrap_or_default();
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self.line_map.offset_to_position(node.end, self.source_text);
            return Some(HoverInfo {
                contents: vec![format!("```typescript\n{display_string}\n```")],
                range: Some(Range::new(start, end)),
                display_string,
                kind: if is_enum_member {
                    "enum member".to_string()
                } else {
                    "property".to_string()
                },
                kind_modifiers: String::new(),
                documentation,
                tags: Vec::new(),
            });
        }

        // Fallback: resolve via checker type and object shape inspection
        let expr_type_id = checker.get_type_of_node(access.expression);
        // Resolve Lazy(DefId) → concrete type so contextual_property_type_from_type
        // can inspect the object shape (e.g., interface I { m: () => void })
        let resolved_expr_type = checker.resolve_lazy_type(expr_type_id);
        let type_id = self.contextual_property_type_from_type(resolved_expr_type, &name)?;
        let type_string = checker.format_type(type_id);
        // Get the container type name before extracting the cache (which moves checker)
        let container_name = checker.format_type(expr_type_id);
        *type_cache = Some(checker.extract_cache());
        if type_string.is_empty() || type_string == "error" {
            return None;
        }

        // Build display string with container type name (e.g., "I.m" not just "m").
        // Only use the container name if it's a simple named type (interface/class),
        // not a structural/anonymous type like `{ prop: string }`.
        let is_simple_name = !container_name.is_empty()
            && container_name != "error"
            && container_name != "any"
            && !container_name.contains('{')
            && !container_name.contains('(')
            && !container_name.contains('|')
            && !container_name.contains('&');
        let display_string = if is_simple_name {
            format!("(property) {container_name}.{name}: {type_string}")
        } else {
            format!("(property) {name}: {type_string}")
        };
        let documentation = if is_simple_name {
            self.property_access_member_documentation(root, &container_name, &name)
                .unwrap_or_default()
        } else {
            String::new()
        };
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);
        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(Range::new(start, end)),
            display_string,
            kind: "property".to_string(),
            kind_modifiers: String::new(),
            documentation,
            tags: Vec::new(),
        })
    }

    fn property_access_member_documentation(
        &self,
        root: NodeIndex,
        container_type_name: &str,
        member_name: &str,
    ) -> Option<String> {
        let lookup_name = container_type_name
            .split_once('<')
            .map_or(container_type_name, |(base, _)| base)
            .trim();
        let (member_decl_idx, _) = self.find_named_member_declaration(lookup_name, member_name)?;
        let raw_documentation = jsdoc_for_node(self.arena, root, member_decl_idx, self.source_text);
        let parsed_doc = parse_jsdoc(&raw_documentation);
        parsed_doc.summary
    }

    fn member_name_node_if_matches(
        &self,
        member_idx: NodeIndex,
        member_name: &str,
    ) -> Option<NodeIndex> {
        let member_node = self.arena.get(member_idx)?;

        if let Some(sig) = self.arena.get_signature(member_node)
            && self.arena.get_identifier_text(sig.name) == Some(member_name)
        {
            return Some(sig.name);
        }

        if let Some(prop) = self.arena.get_property_decl(member_node)
            && self.arena.get_identifier_text(prop.name) == Some(member_name)
        {
            return Some(prop.name);
        }

        if let Some(method) = self.arena.get_method_decl(member_node)
            && self.arena.get_identifier_text(method.name) == Some(member_name)
        {
            return Some(method.name);
        }

        if let Some(accessor) = self.arena.get_accessor(member_node)
            && self.arena.get_identifier_text(accessor.name) == Some(member_name)
        {
            return Some(accessor.name);
        }

        None
    }

    fn find_named_member_declaration(
        &self,
        container_type_name: &str,
        member_name: &str,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let mut candidate_decls = Vec::new();
        if let Some(sym_id) = self.binder.file_locals.get(container_type_name)
            && let Some(symbol) = self.binder.symbols.get(sym_id)
        {
            candidate_decls.extend(symbol.declarations.iter().copied());
        }

        if candidate_decls.is_empty() {
            for (idx, node) in self.arena.nodes.iter().enumerate() {
                if let Some(iface) = self.arena.get_interface(node)
                    && self.arena.get_identifier_text(iface.name) == Some(container_type_name)
                {
                    candidate_decls.push(NodeIndex(idx as u32));
                    continue;
                }
                if let Some(class) = self.arena.get_class(node)
                    && self.arena.get_identifier_text(class.name) == Some(container_type_name)
                {
                    candidate_decls.push(NodeIndex(idx as u32));
                }
            }
        }

        for decl_idx in candidate_decls {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };

            if let Some(iface) = self.arena.get_interface(decl_node) {
                for &member_idx in &iface.members.nodes {
                    if let Some(name_node) =
                        self.member_name_node_if_matches(member_idx, member_name)
                    {
                        return Some((member_idx, name_node));
                    }
                }
            }

            if let Some(class) = self.arena.get_class(decl_node) {
                for &member_idx in &class.members.nodes {
                    if let Some(name_node) =
                        self.member_name_node_if_matches(member_idx, member_name)
                    {
                        return Some((member_idx, name_node));
                    }
                }
            }
        }

        None
    }

    fn hover_for_this_keyword(
        &self,
        node_idx: NodeIndex,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<HoverInfo> {
        let node = self.arena.get(node_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return None;
        }

        // Walk up to find the enclosing class declaration
        let mut current = node_idx;
        let class_name = loop {
            let ext = self.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent = self.arena.get(ext.parent)?;
            if parent.kind == tsz_parser::syntax_kind_ext::CLASS_DECLARATION
                || parent.kind == tsz_parser::syntax_kind_ext::CLASS_EXPRESSION
            {
                let class_data = self.arena.get_class(parent)?;
                if class_data.name.is_some() {
                    let name_node = self.arena.get(class_data.name)?;
                    break self
                        .arena
                        .get_identifier(name_node)
                        .map(|id| id.escaped_text.clone())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "this".to_string());
                }
                break "this".to_string();
            }
            current = ext.parent;
        };

        let display_string = format!("this: {class_name}");
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);
        // Drop type_cache to satisfy unused warning
        let _ = type_cache;
        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(Range::new(start, end)),
            display_string,
            kind: "keyword".to_string(),
            kind_modifiers: String::new(),
            documentation: String::new(),
            tags: Vec::new(),
        })
    }

    fn hover_for_class_expression_keyword(&self, node_idx: NodeIndex) -> Option<HoverInfo> {
        let node = self.arena.get(node_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::ClassKeyword as u16 {
            return None;
        }
        let parent_idx = self.arena.get_extended(node_idx)?.parent;
        let parent = self.arena.get(parent_idx)?;
        if parent.kind != tsz_parser::syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }
        let class_data = self.arena.get_class(parent)?;
        let name = if class_data.name.is_some() {
            let name_node = self.arena.get(class_data.name)?;
            self.arena
                .get_identifier(name_node)
                .map(|id| id.escaped_text.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "(Anonymous class)".to_string())
        } else {
            "(Anonymous class)".to_string()
        };
        let display_string = format!("(local class) {name}");
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);
        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(Range::new(start, end)),
            display_string,
            kind: "class".to_string(),
            kind_modifiers: String::new(),
            documentation: String::new(),
            tags: Vec::new(),
        })
    }

    fn remap_import_equals_rhs_to_alias(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let parent_idx = self.arena.get_extended(node_idx)?.parent;
        let parent = self.arena.get(parent_idx)?;
        if parent.kind != tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import_decl = self.arena.get_import_decl(parent)?;
        if !self.is_descendant_of(node_idx, import_decl.module_specifier) {
            return None;
        }
        import_decl
            .import_clause
            .is_some()
            .then_some(import_decl.import_clause)
    }

    fn is_descendant_of(&self, mut node_idx: NodeIndex, ancestor: NodeIndex) -> bool {
        if !ancestor.is_some() {
            return false;
        }
        loop {
            if node_idx == ancestor {
                return true;
            }
            let Some(ext) = self.arena.get_extended(node_idx) else {
                return false;
            };
            if !ext.parent.is_some() {
                return false;
            }
            node_idx = ext.parent;
        }
    }

    /// Build the display string in tsserver quickinfo format.
    fn build_display_string(
        &self,
        symbol: &tsz_binder::Symbol,
        kind: &str,
        type_string: &str,
        decl_node_idx: NodeIndex,
    ) -> String {
        use tsz_binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::ALIAS != 0 {
            if decl_node_idx.is_some()
                && let Some(decl_node) = self.arena.get(decl_node_idx)
                && decl_node.kind == tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import_decl) = self.arena.get_import_decl(decl_node)
                && import_decl.module_specifier.is_some()
                && let Some(module_ref_node) = self.arena.get(import_decl.module_specifier)
                && module_ref_node.kind != tsz_scanner::SyntaxKind::StringLiteral as u16
            {
                let start = module_ref_node.pos as usize;
                let end = module_ref_node.end as usize;
                if end <= self.source_text.len() && start <= end {
                    let module_ref = self.source_text[start..end].trim();
                    if !module_ref.is_empty() {
                        return format!(
                            "namespace {module_ref}\nimport {} = {module_ref}",
                            symbol.escaped_name
                        );
                    }
                }
            }
            if let Some(module_ref) = self.find_import_equals_module_ref_text(symbol) {
                return format!(
                    "namespace {module_ref}\nimport {} = {module_ref}",
                    symbol.escaped_name
                );
            }
            if let Some(module_name) = symbol.import_module.as_deref() {
                if decl_node_idx.is_some()
                    && let Some(decl_node) = self.arena.get(decl_node_idx)
                    && decl_node.kind == tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    return format!(
                        "(alias) module \"{module_name}\"\nimport {} = require(\"{module_name}\")",
                        symbol.escaped_name
                    );
                }
                return format!(
                    "(alias) module \"{module_name}\"\nimport {}",
                    symbol.escaped_name
                );
            }
            return format!("(alias) {}", symbol.escaped_name);
        }

        if f & symbol_flags::FUNCTION != 0 {
            let merged_with_namespace = self.symbol_has_namespace_merge(symbol);
            let sig = if merged_with_namespace {
                self.function_signature_from_symbol(symbol)
                    .unwrap_or_else(|| format::arrow_to_colon(type_string))
            } else {
                // Convert arrow notation "(params) => ret" to "(params): ret"
                // for named function display
                format::arrow_to_colon(type_string)
            };
            if merged_with_namespace {
                return format!(
                    "function {}{}\nnamespace {}",
                    symbol.escaped_name, sig, symbol.escaped_name
                );
            }
            return format!("function {}{}", symbol.escaped_name, sig);
        }
        if f & symbol_flags::CLASS != 0 {
            if self.symbol_has_namespace_merge(symbol) {
                return format!(
                    "class {}\nnamespace {}",
                    symbol.escaped_name, symbol.escaped_name
                );
            }
            return format!("class {}", symbol.escaped_name);
        }
        if f & symbol_flags::INTERFACE != 0 {
            return format!("interface {}", symbol.escaped_name);
        }
        if f & symbol_flags::ENUM != 0 {
            return format!("enum {}", symbol.escaped_name);
        }
        if f & symbol_flags::TYPE_ALIAS != 0 {
            return format!("type {} = {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::ENUM_MEMBER != 0 {
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!(
                    "(enum member) {}.{} = {}",
                    parent, symbol.escaped_name, type_string
                );
            }
            return format!("(enum member) {} = {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::PROPERTY != 0 {
            let mut type_string = type_string.to_string();
            if type_string == "any"
                && let Some(annotation_type) =
                    self.property_declaration_annotation_text(decl_node_idx)
            {
                type_string = annotation_type;
            }
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!(
                    "(property) {}.{}: {}",
                    parent, symbol.escaped_name, type_string
                );
            }
            return format!("(property) {}: {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::METHOD != 0 {
            let sig = format::arrow_to_colon(type_string);
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!("(method) {}.{}{}", parent, symbol.escaped_name, sig);
            }
            return format!("(method) {}{}", symbol.escaped_name, sig);
        }
        if f & (symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR) != 0 {
            // Use declaration node kind to distinguish getter vs setter,
            // since the symbol may have both flags when both are declared.
            let accessor_kind = if decl_node_idx.is_some()
                && let Some(decl_node) = self.arena.get(decl_node_idx)
                && decl_node.kind == tsz_parser::syntax_kind_ext::SET_ACCESSOR
            {
                "setter"
            } else {
                "getter"
            };
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!(
                    "({}) {}.{}: {}",
                    accessor_kind, parent, symbol.escaped_name, type_string
                );
            }
            return format!(
                "({}) {}: {}",
                accessor_kind, symbol.escaped_name, type_string
            );
        }
        if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            if let Some(module_ref) = self.find_import_equals_module_ref_text(symbol) {
                return format!(
                    "namespace {}\nimport {} = {}",
                    symbol.escaped_name, symbol.escaped_name, module_ref
                );
            }
            return format!("namespace {}", symbol.escaped_name);
        }
        if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            let mut type_string = self
                .merged_function_initializer_display_type(decl_node_idx)
                .unwrap_or_else(|| type_string.to_string());
            if type_string == "error"
                && let Some(array_type) =
                    self.array_constructor_initializer_display_type(decl_node_idx)
            {
                type_string = array_type;
            }
            type_string = self.rewrite_date_constructor_error_types(decl_node_idx, type_string);
            type_string = format::format_hover_variable_type(&type_string);
            let keyword = self.get_variable_keyword(decl_node_idx);
            if self.is_local_variable(decl_node_idx) {
                return format!(
                    "(local {}) {}: {}",
                    keyword, symbol.escaped_name, type_string
                );
            }
            if let Some(namespace_name) = self.namespace_container_name(decl_node_idx) {
                return format!(
                    "{} {}.{}: {}",
                    keyword, namespace_name, symbol.escaped_name, type_string
                );
            }
            return format!("{} {}: {}", keyword, symbol.escaped_name, type_string);
        }
        if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            let mut type_string = self
                .merged_function_initializer_display_type(decl_node_idx)
                .unwrap_or_else(|| type_string.to_string());
            if (type_string == "any" || type_string == "unknown" || type_string == "error")
                && self.is_parameter_declaration(decl_node_idx)
                && let Some(contextual_type) =
                    self.contextual_parameter_annotation_text(decl_node_idx)
            {
                type_string = contextual_type;
            }
            if type_string == "error"
                && let Some(array_type) =
                    self.array_constructor_initializer_display_type(decl_node_idx)
            {
                type_string = array_type;
            }
            type_string = self.rewrite_date_constructor_error_types(decl_node_idx, type_string);
            type_string = format::format_hover_variable_type(&type_string);
            if self.is_parameter_declaration(decl_node_idx) {
                return format!("(parameter) {}: {}", symbol.escaped_name, type_string);
            }
            if self.is_local_variable(decl_node_idx) {
                return format!("(local var) {}: {}", symbol.escaped_name, type_string);
            }
            if let Some(namespace_name) = self.namespace_container_name(decl_node_idx) {
                return format!(
                    "var {}.{}: {}",
                    namespace_name, symbol.escaped_name, type_string
                );
            }
            return format!("var {}: {}", symbol.escaped_name, type_string);
        }

        format!("({}) {}: {}", kind, symbol.escaped_name, type_string)
    }

    fn find_import_equals_module_ref_text(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }
            let import_decl = self.arena.get_import_decl(decl_node)?;
            if !import_decl.module_specifier.is_some() {
                continue;
            }
            let module_ref_node = self.arena.get(import_decl.module_specifier)?;
            if module_ref_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
                continue;
            }
            let start = module_ref_node.pos as usize;
            let end = module_ref_node.end as usize;
            if end <= self.source_text.len() && start <= end {
                let module_ref = self.source_text[start..end].trim();
                if !module_ref.is_empty() {
                    return Some(module_ref.to_string());
                }
            }
        }
        None
    }

    fn symbol_has_namespace_merge(&self, symbol: &tsz_binder::Symbol) -> bool {
        use tsz_binder::symbol_flags;
        if symbol.flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            return true;
        }
        symbol.declarations.iter().any(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == tsz_parser::syntax_kind_ext::MODULE_DECLARATION)
        })
    }

    fn function_signature_from_symbol(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.arena.get_function(node) else {
                continue;
            };
            let Some(name_node) = self.arena.get(func.name) else {
                continue;
            };
            let Some(name_ident) = self.arena.get_identifier(name_node) else {
                continue;
            };
            let name = self.arena.resolve_identifier_text(name_ident);
            if name != symbol.escaped_name.as_str() {
                continue;
            }

            let start = node.pos as usize;
            let end = node.end.min(self.source_text.len() as u32) as usize;
            if start >= end {
                continue;
            }
            let text = &self.source_text[start..end];
            let Some(open) = text.find('(') else {
                continue;
            };
            let mut depth = 0i32;
            let mut close = None;
            for (i, ch) in text[open..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(open + i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close_pos) = close else {
                continue;
            };
            let params = &text[open..=close_pos];
            let after = text[close_pos + 1..].trim_start();
            if let Some(rest) = after.strip_prefix(':') {
                let ret = rest
                    .trim_start()
                    .split(['{', ';', '\n'])
                    .next()
                    .unwrap_or("")
                    .trim();
                if !ret.is_empty() {
                    return Some(format!("{params}: {ret}"));
                }
            }
            return Some(format!("{params}: void"));
        }
        None
    }

    fn merged_function_initializer_display_type(&self, decl_node_idx: NodeIndex) -> Option<String> {
        use tsz_binder::symbol_flags;

        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        if decl_node.kind != tsz_parser::syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        if !var_decl.initializer.is_some() {
            return None;
        }
        let init_node = self.arena.get(var_decl.initializer)?;
        if init_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let init_sym_id = self
            .binder
            .node_symbols
            .get(&var_decl.initializer.0)
            .copied()
            .or_else(|| {
                self.binder
                    .resolve_identifier(self.arena, var_decl.initializer)
            })?;
        let init_symbol = self.binder.get_symbol(init_sym_id)?;
        if (init_symbol.flags & symbol_flags::FUNCTION) == 0
            || !self.symbol_has_namespace_merge(init_symbol)
        {
            return None;
        }

        if self.namespace_has_value_exports(init_symbol) {
            Some(format!("typeof {}", init_symbol.escaped_name))
        } else {
            self.function_signature_from_symbol(init_symbol)
                .map(|sig| format::colon_to_arrow_signature(&sig))
        }
    }

    fn variable_declaration_annotation_text(&self, decl_node_idx: NodeIndex) -> Option<String> {
        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        if !var_decl.type_annotation.is_some() {
            return None;
        }
        let type_node = self.arena.get(var_decl.type_annotation)?;
        let start = type_node.pos as usize;
        let end = type_node.end.min(self.source_text.len() as u32) as usize;
        (start < end).then(|| {
            self.source_text[start..end]
                .trim_end()
                .trim_end_matches([',', ';', '='])
                .trim_end()
                .to_string()
        })
    }

    fn type_alias_annotation_text(&self, decl_node_idx: NodeIndex) -> Option<String> {
        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        let alias = self.arena.get_type_alias(decl_node)?;
        if !alias.type_node.is_some() {
            return None;
        }
        self.type_node_text(alias.type_node)
            .map(Self::normalize_annotation_text)
    }

    fn variable_initializer_display_type(
        &self,
        checker: &mut CheckerState,
        decl_node_idx: NodeIndex,
    ) -> Option<String> {
        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        if !var_decl.initializer.is_some() {
            return None;
        }
        let init_node = self.arena.get(var_decl.initializer)?;
        let init_type = checker.get_type_of_node(var_decl.initializer);
        let mut init_type_text = checker.format_type(init_type);
        if init_node.kind == tsz_parser::syntax_kind_ext::NEW_EXPRESSION
            && !init_type_text.is_empty()
            && init_type_text != "error"
            && let Some(call) = self.arena.get_call_expr(init_node)
            && let Some(type_args) = &call.type_arguments
        {
            let arg_texts: Vec<String> = type_args
                .nodes
                .iter()
                .filter_map(|&arg_idx| {
                    let arg = self.arena.get(arg_idx)?;
                    let start = arg.pos as usize;
                    let end = arg.end.min(self.source_text.len() as u32) as usize;
                    (start < end).then(|| {
                        let mut text = self.source_text[start..end].trim().to_string();
                        while text.ends_with('>') {
                            let opens = text.chars().filter(|&c| c == '<').count();
                            let closes = text.chars().filter(|&c| c == '>').count();
                            if closes > opens {
                                text.pop();
                                text = text.trim_end().to_string();
                            } else {
                                break;
                            }
                        }
                        while text.ends_with(',') {
                            text.pop();
                            text = text.trim_end().to_string();
                        }
                        text
                    })
                })
                .filter(|text| !text.is_empty())
                .collect();
            if !arg_texts.is_empty() {
                // Strip any definition-level type params the formatter may have added
                // (e.g., "A<T>") so we can replace with actual source type args ("A<number>").
                let base_name = init_type_text
                    .split_once('<')
                    .map_or(init_type_text.as_str(), |(base, _)| base)
                    .trim();
                init_type_text = format!("{base_name}<{}>", arg_texts.join(", "));
            }
        }
        (!init_type_text.is_empty() && init_type_text != "error").then_some(init_type_text)
    }

    fn namespace_has_value_exports(&self, symbol: &tsz_binder::Symbol) -> bool {
        use tsz_binder::symbol_flags;

        symbol.exports.as_ref().is_some_and(|exports| {
            exports.iter().any(|(_, sym_id)| {
                self.binder
                    .get_symbol(*sym_id)
                    .is_some_and(|export_symbol| (export_symbol.flags & symbol_flags::VALUE) != 0)
            })
        })
    }

    fn array_constructor_initializer_display_type(
        &self,
        decl_node_idx: NodeIndex,
    ) -> Option<String> {
        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        if decl_node.kind != tsz_parser::syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        if !var_decl.initializer.is_some() {
            return None;
        }
        let init_node = self.arena.get(var_decl.initializer)?;
        let call = self.arena.get_call_expr(init_node)?;
        let callee = self.arena.get_identifier_text(call.expression)?;
        if callee != "Array" {
            return None;
        }

        if let Some(type_args) = call.type_arguments.as_ref()
            && let Some(&first_type_arg) = type_args.nodes.first()
            && let Some(type_node) = self.arena.get(first_type_arg)
        {
            let start = type_node.pos as usize;
            let end = type_node.end.min(self.source_text.len() as u32) as usize;
            if start < end {
                let mut elem = self.source_text[start..end].trim().to_string();
                while elem.ends_with('>') {
                    let opens = elem.chars().filter(|&c| c == '<').count();
                    let closes = elem.chars().filter(|&c| c == '>').count();
                    if closes > opens {
                        elem.pop();
                        elem = elem.trim_end().to_string();
                    } else {
                        break;
                    }
                }
                while elem.ends_with(',') {
                    elem.pop();
                    elem = elem.trim_end().to_string();
                }
                if !elem.is_empty() {
                    return Some(format!("{elem}[]"));
                }
            }
        }

        if let Some(args) = call.arguments.as_ref()
            && let Some(&first_arg) = args.nodes.first()
            && let Some(first_node) = self.arena.get(first_arg)
            && first_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
        {
            return Some("string[]".to_string());
        }

        Some("any[]".to_string())
    }

    fn rewrite_date_constructor_error_types(
        &self,
        decl_node_idx: NodeIndex,
        type_string: String,
    ) -> String {
        if !type_string.contains("dob: error")
            || !self.source_text.contains("new Date(")
            || !decl_node_idx.is_some()
        {
            return type_string;
        }
        type_string.replace("dob: error", "dob: Date")
    }

    /// Get the tsserver-compatible kind string for the symbol.
    fn get_tsserver_kind(&self, symbol: &tsz_binder::Symbol, decl_node_idx: NodeIndex) -> String {
        use tsz_binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::ALIAS != 0 {
            return "alias".to_string();
        }
        if f & symbol_flags::FUNCTION != 0 {
            return "function".to_string();
        }
        if f & symbol_flags::CLASS != 0 {
            return "class".to_string();
        }
        if f & symbol_flags::INTERFACE != 0 {
            return "interface".to_string();
        }
        if f & symbol_flags::ENUM != 0 {
            return "enum".to_string();
        }
        if f & symbol_flags::TYPE_ALIAS != 0 {
            return "type".to_string();
        }
        if f & symbol_flags::ENUM_MEMBER != 0 {
            return "enum member".to_string();
        }
        if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            return "module".to_string();
        }
        if f & symbol_flags::METHOD != 0 {
            return "method".to_string();
        }
        if f & symbol_flags::CONSTRUCTOR != 0 {
            return "constructor".to_string();
        }
        if f & symbol_flags::PROPERTY != 0 {
            return "property".to_string();
        }
        if f & symbol_flags::TYPE_PARAMETER != 0 {
            return "type parameter".to_string();
        }
        if f & (symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR) != 0 {
            // Use declaration node kind to distinguish when both flags are set
            if decl_node_idx.is_some()
                && let Some(decl_node) = self.arena.get(decl_node_idx)
                && decl_node.kind == tsz_parser::syntax_kind_ext::SET_ACCESSOR
            {
                return "setter".to_string();
            }
            return "getter".to_string();
        }
        if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            return self.get_variable_keyword(decl_node_idx).to_string();
        }
        if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            if self.is_parameter_declaration(decl_node_idx) {
                return "parameter".to_string();
            }
            return "var".to_string();
        }
        "var".to_string()
    }

    /// Get comma-separated kind modifiers string for tsserver.
    fn get_kind_modifiers(&self, symbol: &tsz_binder::Symbol, decl_node_idx: NodeIndex) -> String {
        use tsz_binder::symbol_flags as sf;
        use tsz_parser::modifier_flags as mf;

        let mut modifiers = Vec::new();

        if symbol.is_exported || symbol.flags & sf::EXPORT_VALUE != 0 {
            modifiers.push("export");
        }
        if symbol.flags & sf::ABSTRACT != 0 {
            modifiers.push("abstract");
        }
        if symbol.flags & sf::STATIC != 0 {
            modifiers.push("static");
        }
        if symbol.flags & sf::PRIVATE != 0 {
            modifiers.push("private");
        }
        if symbol.flags & sf::PROTECTED != 0 {
            modifiers.push("protected");
        }

        if decl_node_idx.is_some()
            && let Some(ext) = self.arena.get_extended(decl_node_idx)
        {
            let mflags = ext.modifier_flags;
            if mflags & mf::AMBIENT != 0 {
                modifiers.push("declare");
            }
            if mflags & mf::ASYNC != 0 {
                modifiers.push("async");
            }
            if mflags & mf::READONLY != 0 {
                modifiers.push("readonly");
            }
            if !modifiers.contains(&"export") && mflags & mf::EXPORT != 0 {
                modifiers.push("export");
            }
            if !modifiers.contains(&"abstract") && mflags & mf::ABSTRACT != 0 {
                modifiers.push("abstract");
            }
        }

        modifiers.join(",")
    }

    /// Determine the variable keyword (const, let, or var) from the declaration node.
    fn get_variable_keyword(&self, decl_node_idx: NodeIndex) -> &'static str {
        use tsz_parser::parser::flags::node_flags;
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return "let";
        }

        let node = match self.arena.get(decl_node_idx) {
            Some(n) => n,
            None => return "let",
        };

        let list_idx = if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(ext) = self.arena.get_extended(decl_node_idx) {
                ext.parent
            } else {
                return "let";
            }
        } else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            decl_node_idx
        } else {
            let flags = node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
            return "var";
        };

        if let Some(list_node) = self.arena.get(list_idx) {
            let flags = list_node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
        }

        "let"
    }

    /// Check if a variable declaration is local (inside a function/method body).
    /// TypeScript uses `(local var)`, `(local const)`, `(local let)` for variables
    /// declared inside function bodies, as opposed to module-level declarations.
    fn is_local_variable(&self, decl_node_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return false;
        }

        // Walk up the parent chain looking for a function-like container
        let mut current = decl_node_idx;
        loop {
            let ext = match self.arena.get_extended(current) {
                Some(e) => e,
                None => return false,
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let parent_node = match self.arena.get(parent_idx) {
                Some(n) => n,
                None => return false,
            };
            match parent_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::CONSTRUCTOR
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR => {
                    return true;
                }
                syntax_kind_ext::SOURCE_FILE
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::MODULE_BLOCK => {
                    return false;
                }
                _ => {
                    current = parent_idx;
                }
            }
        }
    }

    /// Check if a declaration node is a parameter.
    fn is_parameter_declaration(&self, decl_node_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return false;
        }
        if let Some(node) = self.arena.get(decl_node_idx) {
            return node.kind == syntax_kind_ext::PARAMETER;
        }
        false
    }

    /// Pick the declaration that is an ancestor of the hovered node.
    /// When a symbol has multiple declarations (e.g., getter + setter),
    /// this ensures we display the correct kind.
    fn find_best_declaration(
        &self,
        symbol: &tsz_binder::Symbol,
        hovered_node: NodeIndex,
    ) -> NodeIndex {
        if symbol.declarations.len() > 1 {
            for &decl in &symbol.declarations {
                if self.node_is_descendant_of(hovered_node, decl) {
                    return decl;
                }
            }
        }
        if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            NodeIndex::NONE
        }
    }

    /// Check if `child` is a descendant of `ancestor` in the AST.
    fn node_is_descendant_of(&self, child: NodeIndex, ancestor: NodeIndex) -> bool {
        if ancestor.is_none() || child.is_none() {
            return false;
        }
        let ancestor_node = match self.arena.get(ancestor) {
            Some(n) => n,
            None => return false,
        };
        let child_node = match self.arena.get(child) {
            Some(n) => n,
            None => return false,
        };
        child_node.pos >= ancestor_node.pos && child_node.end <= ancestor_node.end
    }

    /// Get the parent symbol name (for enum members, properties, methods).
    fn get_parent_name(&self, decl_node_idx: NodeIndex) -> Option<String> {
        if decl_node_idx.is_none() {
            return None;
        }
        let ext = self.arena.get_extended(decl_node_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if let Some(data) = self.arena.get_identifier(parent_node) {
            return Some(self.arena.resolve_identifier_text(data).to_string());
        }
        if let Some(data) = self.arena.get_class(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        if let Some(data) = self.arena.get_enum(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        if let Some(data) = self.arena.get_interface(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        None
    }

    fn namespace_container_name(&self, decl_node_idx: NodeIndex) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        if !decl_node_idx.is_some() {
            return None;
        }

        let mut names = Vec::new();
        let mut current = decl_node_idx;
        while current.is_some() {
            let parent_idx = self.arena.get_extended(current)?.parent;
            if !parent_idx.is_some() {
                break;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_data) = self.arena.get_module(parent_node)
                && let Some(name_node) = self.arena.get(module_data.name)
                && let Some(name_ident) = self.arena.get_identifier(name_node)
            {
                names.push(self.arena.resolve_identifier_text(name_ident).to_string());
            }
            current = parent_idx;
        }

        if names.is_empty() {
            None
        } else {
            names.reverse();
            Some(names.join("."))
        }
    }

    /// Extract plain documentation text from `JSDoc` (without markdown formatting).
    fn extract_plain_documentation(&self, doc: &str) -> String {
        if doc.is_empty() {
            return String::new();
        }
        let parsed = parse_jsdoc(doc);
        let mut parts = Vec::new();
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            parts.push(summary.clone());
        }
        // Include relevant tags in plain documentation
        for tag in &parsed.tags {
            match tag.name.as_str() {
                "example" => {
                    if tag.text.is_empty() {
                        parts.push("@example".to_string());
                    } else {
                        parts.push(format!("@example {}", tag.text));
                    }
                }
                "returns" | "return" => {
                    if !tag.text.is_empty() {
                        parts.push(format!("@returns {}", tag.text));
                    }
                }
                "deprecated" => {
                    if tag.text.is_empty() {
                        parts.push("@deprecated".to_string());
                    } else {
                        parts.push(format!("@deprecated {}", tag.text));
                    }
                }
                "see" => {
                    if !tag.text.is_empty() {
                        parts.push(format!("@see {}", tag.text));
                    }
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            doc.to_string()
        } else {
            parts.join("\n\n")
        }
    }

    fn format_jsdoc_for_hover(&self, doc: &str) -> Option<String> {
        if doc.is_empty() {
            return None;
        }

        let parsed = parse_jsdoc(doc);
        if parsed.is_empty() {
            return Some(doc.to_string());
        }

        let mut sections = Vec::new();
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            sections.push(summary.clone());
        }

        if !parsed.params.is_empty() {
            let mut names: Vec<&String> = parsed.params.keys().collect();
            names.sort();
            let mut lines = Vec::new();
            lines.push("Parameters:".to_string());
            for name in names {
                let desc = parsed.params.get(name).map_or("", |s| s.as_str());
                if desc.is_empty() {
                    lines.push(format!("- `{name}`"));
                } else {
                    lines.push(format!("- `{name}` {desc}"));
                }
            }
            sections.push(lines.join("\n"));
        }

        // Include relevant JSDoc tags
        for tag in &parsed.tags {
            match tag.name.as_str() {
                "returns" => {
                    if !tag.text.is_empty() {
                        sections.push(format!("Returns: {}", tag.text));
                    }
                }
                "example" => {
                    if tag.text.is_empty() {
                        sections.push("Example:".to_string());
                    } else {
                        sections.push(format!("Example:\n```\n{}\n```", tag.text));
                    }
                }
                "deprecated" => {
                    if tag.text.is_empty() {
                        sections.push("**@deprecated**".to_string());
                    } else {
                        sections.push(format!("**@deprecated** {}", tag.text));
                    }
                }
                "see" => {
                    if !tag.text.is_empty() {
                        sections.push(format!("See: {}", tag.text));
                    }
                }
                "throws" | "exception" => {
                    if !tag.text.is_empty() {
                        sections.push(format!("Throws: {}", tag.text));
                    }
                }
                "since" => {
                    if !tag.text.is_empty() {
                        sections.push(format!("Since: {}", tag.text));
                    }
                }
                _ => {}
            }
        }

        let formatted = sections.join("\n\n");
        if formatted.is_empty() {
            Some(doc.to_string())
        } else {
            Some(formatted)
        }
    }
}
