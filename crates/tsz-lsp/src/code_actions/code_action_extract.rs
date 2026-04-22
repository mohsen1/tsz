//! Extract Variable refactoring for the LSP.
//!
//! Extracts a selected expression into a new `const` variable declaration
//! inserted before the enclosing statement, replacing the original expression
//! with the variable name.

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::{ScopeId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::{Position, Range};

impl<'a> CodeActionProvider<'a> {
    /// Extract the selected expression to a new variable.
    ///
    /// Example: Selecting `foo.bar.baz` produces:
    /// ```typescript
    /// const extracted = foo.bar.baz;
    /// // ... use extracted here
    /// ```
    pub fn extract_variable(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        // 1. Convert range to offsets
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let end_offset = self.line_map.position_to_offset(range.end, self.source)?;

        // 2. Find the expression node that matches this range
        let expr_idx = self.find_expression_at_range(root, start_offset, end_offset)?;

        // 3. Verify it's an expression (not a statement or declaration)
        let expr_node = self.arena.get(expr_idx)?;
        if !self.is_extractable_expression(expr_node.kind) {
            return None;
        }
        if self.is_assignment_expression(expr_node) || self.is_super_call_expression(expr_node) {
            return None;
        }

        // 4. Find the enclosing statement to determine where to insert the variable
        let stmt_idx = self.find_enclosing_statement(root, expr_idx)?;
        let stmt_node = self.arena.get(stmt_idx)?;
        if !self.statement_allows_lexical_insertion(stmt_idx) {
            return None;
        }
        if !self.expression_and_statement_share_scope(expr_idx, stmt_idx) {
            return None;
        }
        if self.extraction_has_tdz_violation(expr_idx, stmt_idx) {
            return None;
        }

        // 5. Generate a unique variable name scoped to the insertion point.
        let var_name = self.unique_extracted_name(stmt_idx);

        // 6. Extract the selected text (snap to node boundaries)
        let (node_start, node_end) = self.expression_text_span(expr_idx, expr_node);
        let selected_text = self.source.get(node_start as usize..node_end as usize)?;
        let initializer_text = self.format_extracted_initializer(expr_node, selected_text);
        let replacement_range = Range::new(
            self.line_map.offset_to_position(node_start, self.source),
            self.line_map.offset_to_position(node_end, self.source),
        );

        // 7. Create text edits:
        //    a) Insert variable declaration before the statement
        //    b) Replace the selected expression with the variable name

        // Get the position to insert the variable declaration
        let stmt_pos = self.line_map.offset_to_position(stmt_node.pos, self.source);
        let insert_pos = Position::new(stmt_pos.line, 0);

        // Calculate indentation by looking at the statement's line
        let indent = self.get_indentation_at_position(&stmt_pos);

        let declaration = format!("{indent}const {var_name} = {initializer_text};\n");

        let mut edits = Vec::new();

        // Insert the declaration
        edits.push(TextEdit {
            range: Range {
                start: insert_pos,
                end: insert_pos,
            },
            new_text: declaration,
        });

        // Replace the expression with the variable name
        let mut replacement_text = if self.needs_jsx_expression_wrapper(expr_idx) {
            format!("{{{var_name}}}")
        } else {
            var_name.clone()
        };
        if self.should_preserve_parenthesized_replacement(expr_node) {
            replacement_text = format!("({replacement_text})");
        }
        edits.push(TextEdit {
            range: replacement_range,
            new_text: replacement_text,
        });

        // Create the workspace edit
        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), edits);

        Some(CodeAction {
            title: format!("Extract to constant '{var_name}'"),
            kind: CodeActionKind::RefactorExtract,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: None,
        })
    }

    fn unique_extracted_name(&self, stmt_idx: NodeIndex) -> String {
        let mut names = FxHashSet::default();
        if let Some(scope_id) = self.find_enclosing_scope_id(stmt_idx) {
            self.collect_scope_names(scope_id, &mut names);
        }

        let base = "extracted";
        if !names.contains(base) {
            return base.to_string();
        }

        let mut suffix = 2;
        loop {
            let candidate = format!("{base}{suffix}");
            if !names.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn format_extracted_initializer(&self, expr_node: &Node, selected_text: &str) -> String {
        if self.needs_parentheses_for_extraction(expr_node) {
            if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return selected_text.to_string();
            }
            return format!("({selected_text})");
        }
        selected_text.to_string()
    }

    fn expression_text_span(&self, expr_idx: NodeIndex, expr_node: &Node) -> (u32, u32) {
        let mut start = expr_node.pos;
        let mut end = expr_node.end;

        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(expr_node)
        {
            start = self.arena.get(binary.left).map_or(start, |node| node.pos);
            end = self.arena.get(binary.right).map_or(end, |node| node.end);
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
            && let Some(name_node) = self.arena.get(access.name_or_argument)
        {
            end = name_node.end;
        }

        if let Some(ext) = self.arena.get_extended(expr_idx)
            && let Some(parent_node) = self.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
        {
            let parent_start = parent_node.pos as usize;
            let parent_end = parent_node.end as usize;
            if let Some(slice) = self.source.get(parent_start..parent_end) {
                if let Some(open_rel) = slice.find('(') {
                    let open_pos = parent_node.pos + open_rel as u32;
                    if start <= open_pos {
                        start = open_pos.saturating_add(1);
                    }
                }
                if let Some(close_rel) = slice.rfind(')') {
                    let close_pos = parent_node.pos + close_rel as u32;
                    if end > close_pos {
                        end = close_pos;
                    }
                }
            }
        }

        (start, end)
    }

    fn needs_parentheses_for_extraction(&self, expr_node: &Node) -> bool {
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.arena.get_parenthesized(expr_node)
                && let Some(inner) = self.arena.get(paren.expression)
            {
                return self.needs_parentheses_for_extraction(inner);
            }
            return false;
        }

        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(expr_node)
        {
            return binary.operator_token == SyntaxKind::CommaToken as u16;
        }

        false
    }

    fn should_preserve_parenthesized_replacement(&self, expr_node: &Node) -> bool {
        if expr_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }

        let Some(paren) = self.arena.get_parenthesized(expr_node) else {
            return true;
        };
        let Some(inner) = self.arena.get(paren.expression) else {
            return true;
        };

        !self.is_comma_expression(inner)
    }

    fn is_comma_expression(&self, expr_node: &Node) -> bool {
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.arena.get_parenthesized(expr_node)
                && let Some(inner) = self.arena.get(paren.expression)
            {
                return self.is_comma_expression(inner);
            }
            return false;
        }

        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(expr_node)
        {
            return binary.operator_token == SyntaxKind::CommaToken as u16;
        }

        false
    }

    fn needs_jsx_expression_wrapper(&self, expr_idx: NodeIndex) -> bool {
        let node = match self.arena.get(expr_idx) {
            Some(node) => node,
            None => return false,
        };

        if node.kind != syntax_kind_ext::JSX_ELEMENT
            && node.kind != syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
            && node.kind != syntax_kind_ext::JSX_FRAGMENT
        {
            return false;
        }

        let parent = match self.arena.get_extended(expr_idx) {
            Some(ext) => ext.parent,
            None => return false,
        };
        if parent.is_none() {
            return false;
        }

        let parent_node = match self.arena.get(parent) {
            Some(node) => node,
            None => return false,
        };

        parent_node.kind == syntax_kind_ext::JSX_ELEMENT
            || parent_node.kind == syntax_kind_ext::JSX_FRAGMENT
    }

    fn expression_and_statement_share_scope(
        &self,
        expr_idx: NodeIndex,
        stmt_idx: NodeIndex,
    ) -> bool {
        let expr_scope = match self.find_enclosing_scope_id(expr_idx) {
            Some(scope_id) => scope_id,
            None => return true,
        };
        let stmt_scope = match self.find_enclosing_scope_id(stmt_idx) {
            Some(scope_id) => scope_id,
            None => return true,
        };
        expr_scope == stmt_scope
    }

    fn extraction_has_tdz_violation(&self, expr_idx: NodeIndex, stmt_idx: NodeIndex) -> bool {
        let stmt_node = match self.arena.get(stmt_idx) {
            Some(node) => node,
            None => return false,
        };
        let insertion_pos = stmt_node.pos;

        let mut identifiers = Vec::new();
        self.collect_identifier_uses_in_expression(expr_idx, &mut identifiers);
        if identifiers.is_empty() {
            return false;
        }

        let mut seen_symbols = FxHashSet::default();
        for ident_idx in identifiers {
            let Some(sym_id) = self.binder.resolve_identifier(self.arena, ident_idx) else {
                continue;
            };
            if !seen_symbols.insert(sym_id) {
                continue;
            }
            if self.symbol_has_tdz_after(sym_id, insertion_pos) {
                return true;
            }
        }

        false
    }

    fn symbol_has_tdz_after(&self, sym_id: tsz_binder::SymbolId, insertion_pos: u32) -> bool {
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };
        if !self.symbol_is_lexical(symbol.flags) {
            return false;
        }

        let mut earliest_decl: Option<u32> = None;
        for decl_idx in &symbol.declarations {
            let Some(decl_node) = self.arena.get(*decl_idx) else {
                continue;
            };
            earliest_decl = Some(match earliest_decl {
                Some(pos) => pos.min(decl_node.pos),
                None => decl_node.pos,
            });
        }

        matches!(earliest_decl, Some(pos) if pos > insertion_pos)
    }

    const fn symbol_is_lexical(&self, flags: u32) -> bool {
        (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 || (flags & symbol_flags::CLASS) != 0
    }

    fn collect_identifier_uses_in_expression(&self, expr_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        if expr_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(expr_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                out.push(expr_idx);
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.collect_identifier_uses_in_expression(access.expression, out);
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.collect_identifier_uses_in_expression(access.expression, out);
                    self.collect_identifier_uses_in_expression(access.name_or_argument, out);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.collect_identifier_uses_in_expression(call.expression, out);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            self.collect_identifier_uses_in_expression(arg, out);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.arena.get_binary_expr(node) {
                    self.collect_identifier_uses_in_expression(binary.left, out);
                    self.collect_identifier_uses_in_expression(binary.right, out);
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    self.collect_identifier_uses_in_expression(unary.operand, out);
                }
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if node.has_data()
                    && let Some(unary) = self.arena.unary_exprs_ex.get(node.data_index as usize)
                {
                    self.collect_identifier_uses_in_expression(unary.expression, out);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.collect_identifier_uses_in_expression(cond.condition, out);
                    self.collect_identifier_uses_in_expression(cond.when_true, out);
                    self.collect_identifier_uses_in_expression(cond.when_false, out);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.collect_identifier_uses_in_expression(paren.expression, out);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(literal) = self.arena.get_literal_expr(node) {
                    for &elem in &literal.elements.nodes {
                        self.collect_identifier_uses_in_expression(elem, out);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(literal) = self.arena.get_literal_expr(node) {
                    for &elem in &literal.elements.nodes {
                        self.collect_identifier_uses_in_object_literal_element(elem, out);
                    }
                }
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if node.has_data()
                    && let Some(tagged) = self.arena.tagged_templates.get(node.data_index as usize)
                {
                    self.collect_identifier_uses_in_expression(tagged.tag, out);
                    self.collect_identifier_uses_in_expression(tagged.template, out);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = self.arena.get_template_expr(node) {
                    for &span_idx in &template.template_spans.nodes {
                        let Some(span_node) = self.arena.get(span_idx) else {
                            continue;
                        };
                        if let Some(span) = self.arena.get_template_span(span_node) {
                            self.collect_identifier_uses_in_expression(span.expression, out);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if node.has_data()
                    && let Some(assertion) =
                        self.arena.type_assertions.get(node.data_index as usize)
                {
                    self.collect_identifier_uses_in_expression(assertion.expression, out);
                }
            }
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                if let Some(jsx) = self.arena.get_jsx_element(node) {
                    self.collect_identifier_uses_in_jsx_opening(jsx.opening_element, out);
                    for &child in &jsx.children.nodes {
                        self.collect_identifier_uses_in_jsx_child(child, out);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.collect_identifier_uses_in_jsx_opening(expr_idx, out);
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                if let Some(fragment) = self.arena.get_jsx_fragment(node) {
                    for &child in &fragment.children.nodes {
                        self.collect_identifier_uses_in_jsx_child(child, out);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                // Skip nested scopes to avoid capturing non-evaluated identifiers.
            }
            _ => {}
        }
    }

    fn collect_identifier_uses_in_object_literal_element(
        &self,
        element_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(element_node) = self.arena.get(element_idx) else {
            return;
        };

        match element_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(element_node) {
                    self.collect_identifier_uses_in_computed_property_name(prop.name, out);
                    self.collect_identifier_uses_in_expression(prop.initializer, out);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(element_node) {
                    self.collect_identifier_uses_in_computed_property_name(method.name, out);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(element_node) {
                    self.collect_identifier_uses_in_computed_property_name(accessor.name, out);
                }
            }
            _ => {
                self.collect_identifier_uses_in_expression(element_idx, out);
            }
        }
    }

    fn collect_identifier_uses_in_computed_property_name(
        &self,
        name_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(name_node)
        {
            self.collect_identifier_uses_in_expression(computed.expression, out);
        }
    }

    fn collect_identifier_uses_in_jsx_opening(
        &self,
        opening_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(opening_node) = self.arena.get(opening_idx) else {
            return;
        };
        let Some(opening) = self.arena.get_jsx_opening(opening_node) else {
            return;
        };

        self.collect_identifier_uses_in_jsx_tag_name(opening.tag_name, out);
        self.collect_identifier_uses_in_jsx_attributes(opening.attributes, out);
    }

    fn collect_identifier_uses_in_jsx_tag_name(
        &self,
        tag_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(tag_node) = self.arena.get(tag_idx) else {
            return;
        };

        match tag_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(name) = self.arena.get_identifier_text(tag_idx)
                    && Self::jsx_tag_is_component(name)
                {
                    out.push(tag_idx);
                }
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                // Namespaced JSX names are intrinsic; skip to avoid false TDZ positives.
            }
            _ => {
                self.collect_identifier_uses_in_expression(tag_idx, out);
            }
        }
    }

    fn collect_identifier_uses_in_jsx_attributes(
        &self,
        attrs_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(attrs_node) = self.arena.get(attrs_idx) else {
            return;
        };
        let Some(attrs) = self.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &prop in &attrs.properties.nodes {
            let Some(prop_node) = self.arena.get(prop) else {
                continue;
            };
            match prop_node.kind {
                k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                    if let Some(attr) = self.arena.get_jsx_attribute(prop_node) {
                        if attr.initializer.is_none() {
                            continue;
                        }
                        self.collect_identifier_uses_in_jsx_attribute_initializer(
                            attr.initializer,
                            out,
                        );
                    }
                }
                k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                    if let Some(spread) = self.arena.get_jsx_spread_attribute(prop_node) {
                        self.collect_identifier_uses_in_expression(spread.expression, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_identifier_uses_in_jsx_attribute_initializer(
        &self,
        init_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(init_node) = self.arena.get(init_idx) else {
            return;
        };

        if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
            if let Some(expr) = self.arena.get_jsx_expression(init_node) {
                self.collect_identifier_uses_in_expression(expr.expression, out);
            }
            return;
        }

        self.collect_identifier_uses_in_expression(init_idx, out);
    }

    fn collect_identifier_uses_in_jsx_child(&self, child_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let Some(child_node) = self.arena.get(child_idx) else {
            return;
        };

        match child_node.kind {
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                if let Some(expr) = self.arena.get_jsx_expression(child_node) {
                    self.collect_identifier_uses_in_expression(expr.expression, out);
                }
            }
            k if k == syntax_kind_ext::JSX_ELEMENT
                || k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_FRAGMENT =>
            {
                self.collect_identifier_uses_in_expression(child_idx, out);
            }
            _ => {}
        }
    }

    fn jsx_tag_is_component(name: &str) -> bool {
        name.chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
    }

    pub(super) fn find_enclosing_scope_id(&self, node_idx: NodeIndex) -> Option<ScopeId> {
        let mut current = node_idx;
        while current.is_some() {
            if let Some(&scope_id) = self.binder.node_scope_ids.get(&current.0) {
                return Some(scope_id);
            }
            let ext = match self.arena.get_extended(current) {
                Some(ext) => ext,
                None => break,
            };
            current = ext.parent;
        }

        (!self.binder.scopes.is_empty()).then_some(ScopeId(0))
    }

    pub(super) fn collect_scope_names(&self, mut scope_id: ScopeId, names: &mut FxHashSet<String>) {
        while scope_id.is_some() {
            let scope = match self.binder.scopes.get(scope_id.0 as usize) {
                Some(scope) => scope,
                None => break,
            };
            names.extend(scope.table.iter().map(|(name, _)| name.clone()));
            scope_id = scope.parent;
        }
    }

    /// Find an expression node that matches the given range.
    /// Finds the smallest expression node that contains the selection.
    fn find_expression_at_range(
        &self,
        _root: NodeIndex,
        start: u32,
        end: u32,
    ) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, start);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.pos <= start && node.end >= end && self.is_expression(node.kind) {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }

        None
    }

    /// Find the enclosing statement for a given node.
    fn find_enclosing_statement(&self, _root: NodeIndex, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if self.is_statement(node.kind) {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Check if a syntax kind is an expression.
    const fn is_expression(&self, kind: u16) -> bool {
        // Check both token kinds (from scanner) and expression kinds (from parser)
        kind == SyntaxKind::Identifier as u16
            || kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::ThisKeyword as u16
            || kind == SyntaxKind::SuperKeyword as u16
            || kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::CALL_EXPRESSION
            || kind == syntax_kind_ext::BINARY_EXPRESSION
            || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::ARROW_FUNCTION
            || kind == syntax_kind_ext::CLASS_EXPRESSION
            || kind == syntax_kind_ext::NEW_EXPRESSION
            || kind == syntax_kind_ext::TEMPLATE_EXPRESSION
            || kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            || kind == syntax_kind_ext::AWAIT_EXPRESSION
            || kind == syntax_kind_ext::YIELD_EXPRESSION
            || kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            || kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
            || kind == syntax_kind_ext::JSX_ELEMENT
            || kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
            || kind == syntax_kind_ext::JSX_FRAGMENT
    }

    /// Check if an expression is extractable (not all expressions should be extracted).
    const fn is_extractable_expression(&self, kind: u16) -> bool {
        // Don't extract simple literals or identifiers - not useful
        !(kind == SyntaxKind::Identifier as u16
            || kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16)
    }

    fn is_assignment_expression(&self, expr_node: &Node) -> bool {
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }

        let Some(binary) = self.arena.get_binary_expr(expr_node) else {
            return false;
        };

        tsz_solver::is_assignment_operator(binary.operator_token)
    }

    fn is_super_call_expression(&self, expr_node: &Node) -> bool {
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };

        self.arena
            .get(call.expression)
            .is_some_and(|callee| callee.kind == SyntaxKind::SuperKeyword as u16)
    }

    /// Check if a syntax kind is a statement.
    const fn is_statement(&self, kind: u16) -> bool {
        matches!(
            kind,
            syntax_kind_ext::VARIABLE_STATEMENT
                | syntax_kind_ext::EXPRESSION_STATEMENT
                | syntax_kind_ext::IF_STATEMENT
                | syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT
                | syntax_kind_ext::WHILE_STATEMENT
                | syntax_kind_ext::DO_STATEMENT
                | syntax_kind_ext::RETURN_STATEMENT
                | syntax_kind_ext::BREAK_STATEMENT
                | syntax_kind_ext::CONTINUE_STATEMENT
                | syntax_kind_ext::THROW_STATEMENT
                | syntax_kind_ext::TRY_STATEMENT
                | syntax_kind_ext::SWITCH_STATEMENT
                | syntax_kind_ext::BLOCK
        )
    }

    fn statement_allows_lexical_insertion(&self, stmt_idx: NodeIndex) -> bool {
        let parent = match self.arena.get_extended(stmt_idx) {
            Some(ext) => ext.parent,
            None => return false,
        };
        if parent.is_none() {
            return true;
        }

        let parent_node = match self.arena.get(parent) {
            Some(node) => node,
            None => return false,
        };

        matches!(
            parent_node.kind,
            k if k == syntax_kind_ext::SOURCE_FILE
                || k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::MODULE_BLOCK
                || k == syntax_kind_ext::CASE_CLAUSE
                || k == syntax_kind_ext::DEFAULT_CLAUSE
        )
    }
}
