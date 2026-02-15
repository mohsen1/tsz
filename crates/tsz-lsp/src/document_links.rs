//! Document Links implementation for LSP.
//!
//! Provides clickable links for import/export module specifiers in the editor.
//! Walks the AST to find import and export declarations, extracts the module
//! specifier string literals, and returns document links with ranges covering
//! the specifier text (without quotes).

use tsz_common::position::Range;
use tsz_parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// A document link representing a clickable module specifier in the source.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentLink {
    /// The range of the module specifier string (without quotes).
    pub range: Range,
    /// The target URI or raw specifier string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Optional tooltip text shown on hover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

impl DocumentLink {
    /// Create a new document link.
    pub fn new(range: Range, target: Option<String>, tooltip: Option<String>) -> Self {
        Self {
            range,
            target,
            tooltip,
        }
    }
}

define_lsp_provider!(minimal DocumentLinkProvider, "Provider for document links.");

impl<'a> DocumentLinkProvider<'a> {
    /// Provide all document links in the file.
    ///
    /// Walks the AST starting from `root`, finding import/export declarations
    /// and dynamic import / require() calls, then extracts module specifier
    /// strings and builds document links.
    pub fn provide_document_links(&self, root: NodeIndex) -> Vec<DocumentLink> {
        let mut links = Vec::new();
        self.collect_links(root, &mut links);
        links
    }

    /// Recursively collect document links from the AST.
    fn collect_links(&self, node_idx: NodeIndex, links: &mut Vec<DocumentLink>) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            // Source file: recurse into top-level statements.
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        self.collect_links(stmt, links);
                    }
                }
            }

            // import '...' or import x from '...'
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                if let Some(import_data) = self.arena.get_import_decl(node) {
                    self.try_add_link_for_specifier(import_data.module_specifier, links);
                }
            }

            // export { ... } from '...' or export * from '...'
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_data) = self.arena.get_export_decl(node) {
                    self.try_add_link_for_specifier(export_data.module_specifier, links);
                }
            }

            // Call expressions: handle dynamic import() and require()
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.try_add_link_for_call(node_idx, links);
            }

            // Expression statement: recurse into its expression (for bare require() calls)
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_data) = self.arena.get_expression_statement(node) {
                    self.collect_links(expr_data.expression, links);
                }
            }

            // Variable statement: contains a VARIABLE_DECLARATION_LIST child
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    for &child_idx in &var_data.declarations.nodes {
                        self.collect_links(child_idx, links);
                    }
                }
            }

            // Variable declaration list: contains individual VARIABLE_DECLARATION nodes
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    for &decl_idx in &var_data.declarations.nodes {
                        self.collect_links(decl_idx, links);
                    }
                }
            }

            // Variable declaration: check initializer for require() calls
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(var_decl) = self.arena.get_variable_declaration(node) {
                    self.collect_links(var_decl.initializer, links);
                }
            }

            // Block: recurse into block statements
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_links(stmt, links);
                    }
                }
            }

            // Function declarations: recurse into body
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                if let Some(func) = self.arena.get_function(node) {
                    self.collect_links(func.body, links);
                }
            }

            // Module/namespace declaration: recurse into body
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    self.collect_links(module.body, links);
                }
            }

            // Module block: recurse into statements
            k if k == syntax_kind_ext::MODULE_BLOCK => {
                if let Some(module_block) = self.arena.get_module_block(node)
                    && let Some(stmts) = &module_block.statements
                {
                    for &stmt in &stmts.nodes {
                        self.collect_links(stmt, links);
                    }
                }
            }

            _ => {}
        }
    }

    /// Try to create a document link from a module specifier node index.
    /// The node should be a StringLiteral.
    fn try_add_link_for_specifier(&self, specifier_idx: NodeIndex, links: &mut Vec<DocumentLink>) {
        if specifier_idx.is_none() {
            return;
        }

        let Some(spec_node) = self.arena.get(specifier_idx) else {
            return;
        };

        // Must be a string literal
        if spec_node.kind != SyntaxKind::StringLiteral as u16 {
            return;
        }

        let Some(literal_data) = self.arena.get_literal(spec_node) else {
            return;
        };

        let specifier_text = &literal_data.text;

        // Compute the range of the string content (without quotes).
        // The node span includes the quotes, so we offset by 1 from each end.
        let quote_start = spec_node.pos + 1;
        let quote_end = if spec_node.end > 0 {
            spec_node.end - 1
        } else {
            spec_node.end
        };

        // Sanity check: the inner range should be non-negative
        if quote_start > quote_end {
            return;
        }

        let start_pos = self
            .line_map
            .offset_to_position(quote_start, self.source_text);
        let end_pos = self
            .line_map
            .offset_to_position(quote_end, self.source_text);
        let range = Range::new(start_pos, end_pos);

        let tooltip = Some(format!("Open module '{}'", specifier_text));

        links.push(DocumentLink::new(
            range,
            Some(specifier_text.clone()),
            tooltip,
        ));
    }

    /// Try to create a document link from a call expression (dynamic import or require).
    fn try_add_link_for_call(&self, call_idx: NodeIndex, links: &mut Vec<DocumentLink>) {
        let Some(call_node) = self.arena.get(call_idx) else {
            return;
        };

        let Some(call_data) = self.arena.get_call_expr(call_node) else {
            return;
        };

        let is_dynamic_import = self.is_import_keyword(call_data.expression);
        let is_require = self.is_require_identifier(call_data.expression);

        if !is_dynamic_import && !is_require {
            return;
        }

        // Get the first argument, which should be the module specifier string
        let Some(args) = &call_data.arguments else {
            return;
        };

        let Some(&first_arg) = args.nodes.first() else {
            return;
        };

        self.try_add_link_for_specifier(first_arg, links);
    }

    /// Check if a node is the `import` keyword (for dynamic import expressions).
    /// Dynamic imports use SyntaxKind::ImportKeyword as the expression.
    fn is_import_keyword(&self, node_idx: NodeIndex) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        node.kind == SyntaxKind::ImportKeyword as u16
    }

    /// Check if a node is a `require` identifier.
    fn is_require_identifier(&self, node_idx: NodeIndex) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident_data) = self.arena.get_identifier(node) else {
            return false;
        };
        ident_data.escaped_text == "require"
    }
}

#[cfg(test)]
#[path = "../tests/document_links_tests.rs"]
mod document_links_tests;
