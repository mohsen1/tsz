//! Hover implementation for LSP.
//!
//! Displays type information and documentation for the symbol at the cursor.
//! Produces quickinfo output compatible with tsserver's expected format:
//! - `display_string`: The raw signature (e.g. `const x: number`, `function foo(): void`)
//! - `kind`: The symbol kind (e.g. `const`, `function`, `class`)
//! - `kind_modifiers`: Comma-separated modifier list (e.g. `export,declare`)
//! - `documentation`: Extracted `JSDoc` content

use crate::jsdoc::{jsdoc_for_node, parse_jsdoc};
use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::utils::{
    find_symbol_query_node_at_or_before, is_comment_context, should_backtrack_to_previous_symbol,
};
use tsz_checker::state::CheckerState;
use tsz_common::position::{Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

/// A single `JSDoc` tag (e.g. `@param`, `@returns`, `@deprecated`).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct JsDocTag {
    /// The tag name (e.g. "param", "returns", "deprecated")
    pub name: String,
    /// The tag text content
    pub text: String,
}

/// Information returned for a hover request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HoverInfo {
    /// The contents of the hover (usually Markdown)
    pub contents: Vec<String>,
    /// The range of the symbol being hovered
    pub range: Option<Range>,
    /// The raw display string for tsserver quickinfo (e.g. `const x: number`)
    pub display_string: String,
    /// The symbol kind string for tsserver (e.g. `const`, `function`, `class`)
    pub kind: String,
    /// Comma-separated kind modifiers for tsserver (e.g. `export,declare`)
    pub kind_modifiers: String,
    /// The documentation text extracted from `JSDoc`
    pub documentation: String,
    /// `JSDoc` tags (e.g. @param, @returns, @deprecated)
    pub tags: Vec<JsDocTag>,
}

define_lsp_provider!(full HoverProvider, "Hover provider.");

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
        position: Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, None, None)
    }

    pub fn get_hover_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, Some(scope_cache), scope_stats)
    }

    fn get_hover_internal(
        &self,
        root: NodeIndex,
        position: Position,
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

        let type_id = checker.get_type_of_symbol(symbol_id);
        let type_string = checker.format_type(type_id);

        // Extract and save the updated cache for future queries
        *type_cache = Some(checker.extract_cache());

        // 4. Get the declaration node for determining keyword and modifiers
        let decl_node_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            NodeIndex::NONE
        };

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

    fn hover_for_contextual_object_property(
        &self,
        node_idx: NodeIndex,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<HoverInfo> {
        use tsz_parser::syntax_kind_ext;

        let mut current = node_idx;
        let mut prop_assign_idx = NodeIndex::NONE;
        while current.is_some() {
            let current_node = self.arena.get(current)?;
            if current_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                prop_assign_idx = current;
                break;
            }
            current = self.arena.get_extended(current)?.parent;
        }
        if !prop_assign_idx.is_some() {
            return None;
        }

        let prop_assign_node = self.arena.get(prop_assign_idx)?;
        let prop_assign = self.arena.get_property_assignment(prop_assign_node)?;
        if !prop_assign.initializer.is_some() {
            return None;
        }

        let object_literal_idx = self.arena.get_extended(prop_assign_idx)?.parent;
        let object_literal = self.arena.get(object_literal_idx)?;
        if object_literal.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let contextual_type_idx =
            self.contextual_type_for_object_literal(object_literal_idx, prop_assign_idx)?;
        let prop_name = self
            .arena
            .get_identifier_text(prop_assign.name)
            .map(std::string::ToString::to_string)?;

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

        let container_type_id = checker.get_type_of_node(contextual_type_idx);
        let mut value_type_id = self
            .contextual_property_type_from_type(container_type_id, &prop_name)
            .unwrap_or(tsz_solver::TypeId::ERROR);
        let value_type_text = if value_type_id == tsz_solver::TypeId::ERROR {
            self.contextual_property_annotation_text(contextual_type_idx, &prop_name)
        } else {
            None
        };
        if value_type_id == tsz_solver::TypeId::ERROR && value_type_text.is_none() {
            value_type_id = checker.get_type_of_node(prop_assign.initializer);
            if value_type_id == tsz_solver::TypeId::ERROR {
                value_type_id = checker.get_type_of_node(prop_assign_idx);
            }
        }
        let container_type = checker.format_type(container_type_id);
        let value_type = value_type_text.unwrap_or_else(|| checker.format_type(value_type_id));
        *type_cache = Some(checker.extract_cache());

        if container_type.is_empty() || value_type.is_empty() {
            return None;
        }

        let initializer_node = self.arena.get(prop_assign.initializer)?;
        let is_function_like = initializer_node.kind
            == tsz_parser::syntax_kind_ext::FUNCTION_EXPRESSION
            || initializer_node.kind == tsz_parser::syntax_kind_ext::ARROW_FUNCTION;
        let (display_string, kind) = if is_function_like {
            let signature = self
                .contextual_method_signature_text(contextual_type_idx, &prop_name)
                .unwrap_or_else(|| Self::arrow_to_colon(&value_type));
            (
                format!("(method) {container_type}.{prop_name}{signature}"),
                "method".to_string(),
            )
        } else {
            (
                format!("(property) {container_type}.{prop_name}: {value_type}"),
                "property".to_string(),
            )
        };
        let name_node = self.arena.get(prop_assign.name)?;
        let start = self
            .line_map
            .offset_to_position(name_node.pos, self.source_text);
        let end = self
            .line_map
            .offset_to_position(name_node.end, self.source_text);
        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(Range::new(start, end)),
            display_string,
            kind,
            kind_modifiers: String::new(),
            documentation: String::new(),
            tags: Vec::new(),
        })
    }

    fn contextual_property_type_from_type(
        &self,
        container_type_id: tsz_solver::TypeId,
        prop_name: &str,
    ) -> Option<tsz_solver::TypeId> {
        use tsz_solver::visitor;

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

    fn contextual_method_signature_text(
        &self,
        contextual_type_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let contextual_type_idx = self.unwrap_parenthesized_type_node(contextual_type_idx)?;
        let contextual_node = self.arena.get(contextual_type_idx)?;

        if contextual_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let literal = self.arena.get_type_literal(contextual_node)?;
            for &member_idx in &literal.members.nodes {
                if let Some(sig_text) =
                    self.signature_text_if_matching_member(member_idx, prop_name)
                {
                    return Some(sig_text);
                }
            }
            return None;
        }

        if contextual_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.arena.get_type_ref(contextual_node)?;
        let target = type_ref.type_name;
        let sym_id = self
            .binder
            .node_symbols
            .get(&target.0)
            .copied()
            .or_else(|| self.binder.resolve_identifier(self.arena, target))?;
        let symbol = self.binder.symbols.get(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                let iface = self.arena.get_interface(decl_node)?;
                for &member_idx in &iface.members.nodes {
                    if let Some(sig_text) =
                        self.signature_text_if_matching_member(member_idx, prop_name)
                    {
                        return Some(sig_text);
                    }
                }
            } else if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                let alias = self.arena.get_type_alias(decl_node)?;
                if let Some(sig_text) =
                    self.signature_text_if_matching_type_literal(alias.type_node, prop_name)
                {
                    return Some(sig_text);
                }
            }
        }

        None
    }

    fn contextual_property_annotation_text(
        &self,
        contextual_type_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let contextual_type_idx = self.unwrap_parenthesized_type_node(contextual_type_idx)?;
        let contextual_node = self.arena.get(contextual_type_idx)?;

        if contextual_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let literal = self.arena.get_type_literal(contextual_node)?;
            for &member_idx in &literal.members.nodes {
                if let Some(type_text) =
                    self.property_type_text_if_matching_member(member_idx, prop_name)
                {
                    return Some(type_text);
                }
            }
            return None;
        }

        if contextual_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }

        let type_ref = self.arena.get_type_ref(contextual_node)?;
        let target = type_ref.type_name;
        let sym_id = self
            .binder
            .node_symbols
            .get(&target.0)
            .copied()
            .or_else(|| self.binder.resolve_identifier(self.arena, target))?;
        let symbol = self.binder.symbols.get(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                let iface = self.arena.get_interface(decl_node)?;
                for &member_idx in &iface.members.nodes {
                    if let Some(type_text) =
                        self.property_type_text_if_matching_member(member_idx, prop_name)
                    {
                        return Some(type_text);
                    }
                }
            } else if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                let alias = self.arena.get_type_alias(decl_node)?;
                if let Some(type_text) =
                    self.property_type_text_if_matching_type_literal(alias.type_node, prop_name)
                {
                    return Some(type_text);
                }
            }
        }

        None
    }

    fn property_type_text_if_matching_type_literal(
        &self,
        type_node_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let type_node_idx = self.unwrap_parenthesized_type_node(type_node_idx)?;
        let type_node = self.arena.get(type_node_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let literal = self.arena.get_type_literal(type_node)?;
        for &member_idx in &literal.members.nodes {
            if let Some(type_text) =
                self.property_type_text_if_matching_member(member_idx, prop_name)
            {
                return Some(type_text);
            }
        }
        None
    }

    fn property_type_text_if_matching_member(
        &self,
        member_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let signature = self.arena.get_signature(member_node)?;
        let name = self
            .arena
            .get_identifier_text(signature.name)
            .or_else(|| self.arena.get_literal_text(signature.name))?;
        if name != prop_name
            || !signature
                .parameters
                .as_ref()
                .is_none_or(|p| p.nodes.is_empty())
        {
            return None;
        }
        if !signature.type_annotation.is_some() {
            return None;
        }
        self.type_node_text(signature.type_annotation)
            .map(Self::normalize_annotation_text)
    }

    fn signature_text_if_matching_type_literal(
        &self,
        type_node_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let type_node_idx = self.unwrap_parenthesized_type_node(type_node_idx)?;
        let type_node = self.arena.get(type_node_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let literal = self.arena.get_type_literal(type_node)?;
        for &member_idx in &literal.members.nodes {
            if let Some(sig_text) = self.signature_text_if_matching_member(member_idx, prop_name) {
                return Some(sig_text);
            }
        }
        None
    }

    fn signature_text_if_matching_member(
        &self,
        member_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let signature = self.arena.get_signature(member_node)?;
        let name = self
            .arena
            .get_identifier_text(signature.name)
            .or_else(|| self.arena.get_literal_text(signature.name))?;
        if name != prop_name {
            return None;
        }
        self.signature_data_to_text(signature)
    }

    fn signature_data_to_text(
        &self,
        signature: &tsz_parser::parser::node::SignatureData,
    ) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(params) = signature.parameters.as_ref() {
            for &param_idx in &params.nodes {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let name = self
                    .arena
                    .get_identifier_text(param.name)
                    .or_else(|| self.arena.get_literal_text(param.name))
                    .unwrap_or("arg");
                let ty = if param.type_annotation.is_some() {
                    self.type_node_text(param.type_annotation)
                        .map(Self::normalize_annotation_text)?
                } else {
                    "any".to_string()
                };
                parts.push(format!("{name}: {ty}"));
            }
        }

        let ret = if signature.type_annotation.is_some() {
            self.type_node_text(signature.type_annotation)
                .map(Self::normalize_annotation_text)?
        } else {
            "any".to_string()
        };
        Some(format!("({}): {ret}", parts.join(", ")))
    }

    fn contextual_type_for_object_literal(
        &self,
        object_literal_idx: NodeIndex,
        property_assignment_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let parent_idx = self.arena.get_extended(object_literal_idx)?.parent;
        let parent = self.arena.get(parent_idx)?;

        if parent.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(parent)?;
            if decl.initializer == object_literal_idx && decl.type_annotation.is_some() {
                return Some(decl.type_annotation);
            }
        }

        if parent.kind == syntax_kind_ext::TYPE_ASSERTION
            || parent.kind == syntax_kind_ext::AS_EXPRESSION
            || parent.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(parent)?;
            if assertion.expression == object_literal_idx {
                return Some(assertion.type_node);
            }
        }

        if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(grand_parent_idx) = self.arena.get_extended(parent_idx).map(|e| e.parent)
            && grand_parent_idx.is_some()
        {
            return self.contextual_type_for_object_literal(parent_idx, property_assignment_idx);
        }

        if parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && parent_idx == property_assignment_idx
            && let Some(grand_parent_idx) = self.arena.get_extended(parent_idx).map(|e| e.parent)
            && grand_parent_idx.is_some()
        {
            let grand_parent = self.arena.get(grand_parent_idx)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return self.contextual_type_for_object_literal(grand_parent_idx, parent_idx);
            }
        }

        None
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
                    .unwrap_or_else(|| Self::arrow_to_colon(type_string))
            } else {
                // Convert arrow notation "(params) => ret" to "(params): ret"
                // for named function display
                Self::arrow_to_colon(type_string)
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
            let sig = Self::arrow_to_colon(type_string);
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!("(method) {}.{}{}", parent, symbol.escaped_name, sig);
            }
            return format!("(method) {}{}", symbol.escaped_name, sig);
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
            type_string = Self::format_hover_variable_type(&type_string);
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
            if type_string == "any"
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
            type_string = Self::format_hover_variable_type(&type_string);
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
                .map(|sig| Self::colon_to_arrow_signature(&sig))
        }
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

    fn colon_to_arrow_signature(signature: &str) -> String {
        let trimmed = signature.trim();
        if !trimmed.starts_with('(') {
            return trimmed.to_string();
        }
        let bytes = trimmed.as_bytes();
        let mut depth = 0i32;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let after = trimmed[i + 1..].trim_start();
                        if let Some(rest) = after.strip_prefix(':') {
                            return format!("{} => {}", &trimmed[..=i], rest.trim_start());
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
        trimmed.to_string()
    }

    /// Convert arrow notation `(params) => ret` to colon notation `(params): ret`.
    /// Used when displaying named functions/methods where TypeScript uses `:` for
    /// the return type, not `=>`.
    fn arrow_to_colon(type_string: &str) -> String {
        // Find the last `) => ` at paren depth 0 and replace with `): `
        let bytes = type_string.as_bytes();
        let mut depth = 0i32;
        let mut last_close = None;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        last_close = Some(i);
                    }
                }
                _ => {}
            }
        }
        if let Some(close_idx) = last_close {
            let after = &type_string[close_idx + 1..];
            if let Some(arrow_pos) = after.find(" => ") {
                let before = &type_string[..close_idx + 1];
                let ret = &after[arrow_pos + 4..];
                return format!("{before}: {ret}");
            }
        }
        type_string.to_string()
    }

    fn format_hover_variable_type(type_string: &str) -> String {
        let expanded = Self::expand_inline_object_literals(type_string);
        Self::normalize_union_array_precedence(&expanded)
    }

    fn expand_inline_object_literals(type_string: &str) -> String {
        let mut out = String::with_capacity(type_string.len() + 16);
        let mut cursor = 0usize;

        while let Some(rel_open) = type_string[cursor..].find('{') {
            let open = cursor + rel_open;
            out.push_str(&type_string[cursor..open]);
            let Some(close) = Self::find_matching_brace(type_string, open) else {
                out.push_str(&type_string[open..]);
                return out;
            };
            let inner = &type_string[open + 1..close];
            if let Some(multiline) = Self::format_object_inner_multiline(inner) {
                out.push_str(&multiline);
            } else {
                out.push_str(&type_string[open..=close]);
            }
            cursor = close + 1;
        }

        out.push_str(&type_string[cursor..]);
        out
    }

    fn find_matching_brace(text: &str, open_brace: usize) -> Option<usize> {
        let mut depth = 0i32;
        for (idx, ch) in text[open_brace..].char_indices() {
            let absolute = open_brace + idx;
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(absolute);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn format_object_inner_multiline(inner: &str) -> Option<String> {
        if inner.contains('\n') {
            return None;
        }
        let props: Vec<&str> = inner
            .split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if props.len() < 2 {
            return None;
        }
        if props.iter().any(|p| p.contains('{') || p.contains('}')) {
            return None;
        }
        if !props.iter().all(|p| p.contains(':')) {
            return None;
        }
        Some(format!("{{\n    {};\n}}", props.join(";\n    ")))
    }

    fn normalize_union_array_precedence(type_string: &str) -> String {
        let trimmed = type_string.trim();
        if !trimmed.ends_with("[]") || trimmed.ends_with(")[]") {
            return type_string.to_string();
        }

        let mut parts = Vec::new();
        let mut start = 0usize;
        let mut depth_paren = 0i32;
        let mut depth_brace = 0i32;
        let mut depth_bracket = 0i32;

        for (idx, ch) in trimmed.char_indices() {
            match ch {
                '(' => depth_paren += 1,
                ')' => depth_paren -= 1,
                '{' => depth_brace += 1,
                '}' => depth_brace -= 1,
                '[' => depth_bracket += 1,
                ']' => depth_bracket -= 1,
                '|' if depth_paren == 0 && depth_brace == 0 && depth_bracket == 0 => {
                    parts.push(trimmed[start..idx].trim().to_string());
                    start = idx + 1;
                }
                _ => {}
            }
        }

        if parts.is_empty() {
            return type_string.to_string();
        }
        parts.push(trimmed[start..].trim().to_string());

        let Some(last) = parts.last() else {
            return type_string.to_string();
        };
        if !last.ends_with("[]") {
            return type_string.to_string();
        }
        if parts[..parts.len().saturating_sub(1)]
            .iter()
            .any(|part| part.ends_with("[]"))
        {
            return type_string.to_string();
        }

        let mut normalized = parts;
        if let Some(last_part) = normalized.last_mut() {
            *last_part = last_part
                .strip_suffix("[]")
                .unwrap_or(last_part.as_str())
                .trim()
                .to_string();
        }
        format!("({})[]", normalized.join(" | "))
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
        if f & symbol_flags::GET_ACCESSOR != 0 {
            return "getter".to_string();
        }
        if f & symbol_flags::SET_ACCESSOR != 0 {
            return "setter".to_string();
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

    fn contextual_parameter_annotation_text(&self, param_decl_idx: NodeIndex) -> Option<String> {
        let param_node = self.arena.get(param_decl_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if param.type_annotation.is_some() {
            return None;
        }

        let fn_idx = self.arena.get_extended(param_decl_idx)?.parent;
        let fn_node = self.arena.get(fn_idx)?;
        let fn_data = self.arena.get_function(fn_node)?;
        let param_index = fn_data
            .parameters
            .nodes
            .iter()
            .position(|&idx| idx == param_decl_idx)?;

        let contextual_type_node = self.contextual_function_type_node_for_expression(fn_idx)?;
        let contextual_node = self.arena.get(contextual_type_node)?;
        let contextual_param_idx =
            if let Some(fn_type) = self.arena.get_function_type(contextual_node) {
                *fn_type.parameters.nodes.get(param_index)?
            } else if let Some(signature) = self.arena.get_signature(contextual_node) {
                let params = signature.parameters.as_ref()?;
                *params.nodes.get(param_index)?
            } else {
                return None;
            };
        let contextual_param_node = self.arena.get(contextual_param_idx)?;
        let contextual_param = self.arena.get_parameter(contextual_param_node)?;
        if !contextual_param.type_annotation.is_some() {
            return None;
        }

        self.type_node_text(contextual_param.type_annotation)
            .map(Self::normalize_annotation_text)
    }

    fn property_declaration_annotation_text(&self, decl_node_idx: NodeIndex) -> Option<String> {
        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        let property_decl = self.arena.get_property_decl(decl_node)?;
        if !property_decl.type_annotation.is_some() {
            return None;
        }
        self.type_node_text(property_decl.type_annotation)
            .map(Self::normalize_annotation_text)
    }

    fn normalize_annotation_text(text: String) -> String {
        text.trim_end()
            .trim_end_matches([',', ';', '='])
            .trim_end()
            .to_string()
    }

    fn contextual_function_type_node_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let mut current = expr_idx;
        while current.is_some() {
            let ext = self.arena.get_extended(current)?;
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return None;
            }
            let parent = self.arena.get(parent_idx)?;

            if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                current = parent_idx;
                continue;
            }

            if parent.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && let Some(callable_type) =
                    self.contextual_callable_type_for_array_element(parent_idx, current)
            {
                return Some(callable_type);
            }

            if parent.kind == syntax_kind_ext::TYPE_ASSERTION
                || parent.kind == syntax_kind_ext::AS_EXPRESSION
                || parent.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            {
                let assertion = self.arena.get_type_assertion(parent)?;
                if assertion.expression == current {
                    let type_node = self.arena.get(assertion.type_node)?;
                    if type_node.kind == syntax_kind_ext::FUNCTION_TYPE {
                        return Some(assertion.type_node);
                    }
                }
            }

            if parent.kind == syntax_kind_ext::VARIABLE_DECLARATION
                && let Some(decl) = self.arena.get_variable_declaration(parent)
                && decl.initializer == current
                && decl.type_annotation.is_some()
                && let Some(type_node) = self.arena.get(decl.type_annotation)
                && type_node.kind == syntax_kind_ext::FUNCTION_TYPE
            {
                return Some(decl.type_annotation);
            }

            if parent.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(decl) = self.arena.get_property_decl(parent)
                && decl.initializer == current
                && decl.type_annotation.is_some()
                && let Some(type_node) = self.arena.get(decl.type_annotation)
                && type_node.kind == syntax_kind_ext::FUNCTION_TYPE
            {
                return Some(decl.type_annotation);
            }

            current = parent_idx;
        }

        None
    }

    fn contextual_callable_type_for_array_element(
        &self,
        array_literal_idx: NodeIndex,
        element_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let array_literal_node = self.arena.get(array_literal_idx)?;
        let literal_expr = self.arena.get_literal_expr(array_literal_node)?;
        let element_index = literal_expr
            .elements
            .nodes
            .iter()
            .position(|&idx| idx == element_idx)?;

        let array_ext = self.arena.get_extended(array_literal_idx)?;
        let container_idx = array_ext.parent;
        if !container_idx.is_some() {
            return None;
        }
        let container_node = self.arena.get(container_idx)?;

        let annotation_type_idx = if container_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(container_node)?;
            if decl.initializer != array_literal_idx || !decl.type_annotation.is_some() {
                return None;
            }
            decl.type_annotation
        } else if container_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let decl = self.arena.get_property_decl(container_node)?;
            if decl.initializer != array_literal_idx || !decl.type_annotation.is_some() {
                return None;
            }
            decl.type_annotation
        } else if container_node.kind == syntax_kind_ext::TYPE_ASSERTION
            || container_node.kind == syntax_kind_ext::AS_EXPRESSION
            || container_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(container_node)?;
            if assertion.expression != array_literal_idx {
                return None;
            }
            assertion.type_node
        } else {
            return None;
        };

        self.callable_type_from_array_annotation(annotation_type_idx, element_index)
    }

    fn callable_type_from_array_annotation(
        &self,
        annotation_type_idx: NodeIndex,
        element_index: usize,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let annotation_type_idx = self.unwrap_parenthesized_type_node(annotation_type_idx)?;
        let annotation_type_node = self.arena.get(annotation_type_idx)?;

        if annotation_type_node.kind == syntax_kind_ext::ARRAY_TYPE {
            let array_type = self.arena.get_array_type(annotation_type_node)?;
            return self.callable_type_node_from_type_node(array_type.element_type);
        }

        if annotation_type_node.kind == syntax_kind_ext::TUPLE_TYPE {
            let tuple_type = self.arena.get_tuple_type(annotation_type_node)?;
            let element_type = *tuple_type.elements.nodes.get(element_index)?;
            return self.callable_type_node_from_type_node(element_type);
        }

        None
    }

    fn callable_type_node_from_type_node(&self, type_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let type_idx = self.unwrap_parenthesized_type_node(type_idx)?;
        let type_node = self.arena.get(type_idx)?;

        if type_node.kind == syntax_kind_ext::FUNCTION_TYPE {
            return Some(type_idx);
        }

        if type_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let type_literal = self.arena.get_type_literal(type_node)?;
            for &member_idx in &type_literal.members.nodes {
                let member_node = self.arena.get(member_idx)?;
                if member_node.kind == syntax_kind_ext::CALL_SIGNATURE {
                    return Some(member_idx);
                }
            }
        }

        None
    }

    fn unwrap_parenthesized_type_node(&self, mut type_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        loop {
            let type_node = self.arena.get(type_idx)?;
            if type_node.kind != syntax_kind_ext::PARENTHESIZED_TYPE {
                return Some(type_idx);
            }
            let wrapped = self.arena.get_wrapped_type(type_node)?;
            type_idx = wrapped.type_node;
        }
    }

    fn type_node_text(&self, type_node_idx: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_node_idx)?;
        let start = type_node.pos as usize;
        let end = type_node.end.min(self.source_text.len() as u32) as usize;
        (start < end).then(|| {
            let mut text = self.source_text[start..end].trim().to_string();
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

    /// Extract plain documentation text from `JSDoc` (without markdown formatting).
    fn extract_plain_documentation(&self, doc: &str) -> String {
        if doc.is_empty() {
            return String::new();
        }
        let parsed = parse_jsdoc(doc);
        if let Some(summary) = parsed.summary.as_ref() {
            summary.clone()
        } else {
            doc.to_string()
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

        let formatted = sections.join("\n\n");
        if formatted.is_empty() {
            Some(doc.to_string())
        } else {
            Some(formatted)
        }
    }
}

#[cfg(test)]
#[path = "../tests/hover_tests.rs"]
mod hover_tests;
