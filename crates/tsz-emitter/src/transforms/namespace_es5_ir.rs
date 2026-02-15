//! ES5 Namespace Transform (IR-based)
//!
//! Transforms TypeScript namespaces to ES5 IIFE patterns, producing IR nodes.
//!
//! # Architecture
//!
//! This module provides two main types:
//! - `NamespaceES5Transformer`: The main transformer struct that produces IR nodes
//! - `NamespaceTransformContext`: A context helper for namespace transformations
//!
//! # Examples
//!
//! Simple namespace:
//! ```typescript
//! namespace foo {
//!     export class Provide { }
//! }
//! ```
//!
//! Becomes IR that prints as:
//! ```javascript
//! var foo;
//! (function (foo) {
//!     var Provide = /** @class */ (function () {
//!         function Provide() { }
//!         return Provide;
//!     }());
//!     foo.Provide = Provide;
//! })(foo || (foo = {}));
//! ```
//!
//! Qualified namespace name (A.B.C) produces nested IIFEs:
//! ```typescript
//! namespace A.B.C {
//!     export const x = 1;
//! }
//! ```
//!
//! Becomes:
//! ```javascript
//! var A;
//! (function (A) {
//!     var B;
//!     (function (B) {
//!         var C;
//!         (function (C) {
//!             var x = 1;
//!             C.x = x;
//!         })(C = B.C || (B.C = {}));
//!     })(B = A.B || (A.B = {}));
//! })(A || (A = {}));
//! ```

use crate::transforms::class_es5_ir::{AstToIr, ES5ClassTransformer};
use crate::transforms::enum_es5_ir::transform_enum_to_ir;
use crate::transforms::ir::*;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

// =============================================================================
// NamespaceES5Transformer - Main transformer struct
// =============================================================================

/// ES5 Namespace Transformer
///
/// Transforms TypeScript namespace declarations into ES5-compatible IIFE patterns.
/// This is the primary entry point for namespace IR transformations.
///
/// # Example
///
/// ```ignore
/// use crate::transforms::namespace_es5_ir::NamespaceES5Transformer;
/// use crate::transforms::ir_printer::IRPrinter;
///
/// let transformer = NamespaceES5Transformer::new(&arena);
/// if let Some(ir) = transformer.transform_namespace(ns_idx) {
///     let output = IRPrinter::emit_to_string(&ir);
/// }
/// ```
pub struct NamespaceES5Transformer<'a> {
    arena: &'a NodeArena,
    is_commonjs: bool,
    source_text: Option<&'a str>,
    comment_ranges: Vec<tsz_common::comments::CommentRange>,
}

impl<'a> NamespaceES5Transformer<'a> {
    /// Create a new namespace transformer
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            is_commonjs: false,
            source_text: None,
            comment_ranges: Vec::new(),
        }
    }

    /// Create a namespace transformer with CommonJS mode enabled
    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        Self {
            arena,
            is_commonjs,
            source_text: None,
            comment_ranges: Vec::new(),
        }
    }

    /// Set source text for comment extraction
    pub fn set_source_text(&mut self, text: &'a str) {
        self.comment_ranges = tsz_common::comments::get_comment_ranges(text);
        self.source_text = Some(text);
    }

    /// Set CommonJS mode
    pub fn set_commonjs(&mut self, is_commonjs: bool) {
        self.is_commonjs = is_commonjs;
    }

    /// Extract leading comments from source text that fall within [from_pos, to_pos) range.
    /// Returns IRNode::Raw nodes since the text already includes comment delimiters.
    fn extract_comments_in_range(&self, from_pos: u32, to_pos: u32) -> Vec<IRNode> {
        let source_text = match self.source_text {
            Some(t) => t,
            None => return Vec::new(),
        };
        let mut result = Vec::new();
        for c in &self.comment_ranges {
            if c.pos >= from_pos && c.end <= to_pos {
                let text = c.get_text(source_text);
                if !text.is_empty() {
                    result.push(IRNode::Raw(text.to_string()));
                }
            }
            if c.pos >= to_pos {
                break; // Comments are sorted by position
            }
        }
        result
    }

    /// Skip whitespace and comments forward from `pos` to find the actual token start.
    /// Returns the position of the first non-trivia character.
    fn skip_trivia_forward(&self, pos: u32, end: u32) -> u32 {
        let source_text = match self.source_text {
            Some(t) => t,
            None => return pos,
        };
        let bytes = source_text.as_bytes();
        let mut i = pos as usize;
        let end = end as usize;
        while i < end && i < bytes.len() {
            match bytes[i] {
                b' ' | b'\t' | b'\n' | b'\r' => i += 1,
                b'/' if i + 1 < end => {
                    if bytes[i + 1] == b'/' {
                        // Line comment: skip to end of line
                        i += 2;
                        while i < end && i < bytes.len() && bytes[i] != b'\n' {
                            i += 1;
                        }
                        if i < end && i < bytes.len() && bytes[i] == b'\n' {
                            i += 1;
                        }
                    } else if bytes[i + 1] == b'*' {
                        // Block comment: skip to */
                        i += 2;
                        while i + 1 < end && i + 1 < bytes.len() {
                            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        i as u32
    }

    /// Find the position after the code content of an erased statement (interface/type alias).
    /// Scans forward with brace-depth tracking to find the closing `}` or `;`.
    /// This is needed because `node.end` includes trailing trivia that may contain
    /// comments belonging to the next statement.
    fn find_code_end_of_erased_stmt(&self, node_pos: u32, node_end: u32) -> u32 {
        let source_text = match self.source_text {
            Some(t) => t,
            None => return node_end,
        };
        let bytes = source_text.as_bytes();
        let end = (node_end as usize).min(bytes.len());
        let mut i = node_pos as usize;
        let mut brace_depth: i32 = 0;
        let mut found_brace = false;

        while i < end {
            // Skip over comment ranges
            let pos = i as u32;
            let mut skipped_comment = false;
            for c in &self.comment_ranges {
                if c.pos <= pos && pos < c.end {
                    i = c.end as usize;
                    skipped_comment = true;
                    break;
                }
                if c.pos > pos {
                    break; // comments sorted by position
                }
            }
            if skipped_comment {
                continue;
            }

            match bytes[i] {
                b'{' => {
                    brace_depth += 1;
                    found_brace = true;
                }
                b'}' => {
                    brace_depth -= 1;
                    if found_brace && brace_depth == 0 {
                        return (i + 1) as u32;
                    }
                }
                b';' if brace_depth == 0 && !found_brace => {
                    // Type alias without braces: type Foo = number;
                    return (i + 1) as u32;
                }
                b'\'' | b'"' => {
                    // Skip string literal
                    let quote = bytes[i];
                    i += 1;
                    while i < end && bytes[i] != quote {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                    if i < end {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
            i += 1;
        }

        node_end
    }

    /// Extract standalone comments (on their own line) within [from_pos, to_pos).
    /// Unlike `extract_comments_in_range`, this filters out trailing comments
    /// that share a line with code — only comments on their own line are returned.
    fn extract_standalone_comments_in_range(&self, from_pos: u32, to_pos: u32) -> Vec<IRNode> {
        let source_text = match self.source_text {
            Some(t) => t,
            None => return Vec::new(),
        };
        let bytes = source_text.as_bytes();
        let mut result = Vec::new();
        for c in &self.comment_ranges {
            if c.pos >= from_pos && c.end <= to_pos {
                // Check if standalone: only whitespace before it on the line
                let mut line_start = c.pos as usize;
                while line_start > 0
                    && bytes[line_start - 1] != b'\n'
                    && bytes[line_start - 1] != b'\r'
                {
                    line_start -= 1;
                }
                let before = &source_text[line_start..c.pos as usize];
                if before.trim().is_empty() {
                    let text = c.get_text(source_text);
                    if !text.is_empty() {
                        result.push(IRNode::Raw(text.to_string()));
                    }
                }
            }
            if c.pos >= to_pos {
                break;
            }
        }
        result
    }

    /// Extract a trailing comment within a statement's span.
    ///
    /// In our parser, `node.end` includes trailing trivia, so comments appear
    /// WITHIN `[stmt_pos, stmt_end)` rather than after `stmt_end`. This method
    /// finds comments within the span that have code on the same line before them
    /// (i.e., they're trailing comments, not standalone leading comments).
    fn extract_trailing_comment_in_stmt(&self, stmt_pos: u32, stmt_end: u32) -> Option<String> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();

        for c in &self.comment_ranges {
            if c.pos >= stmt_pos && c.end <= stmt_end {
                // Check if there's non-whitespace code before this comment on the same line
                let mut line_start = c.pos as usize;
                while line_start > 0
                    && bytes[line_start - 1] != b'\n'
                    && bytes[line_start - 1] != b'\r'
                {
                    line_start -= 1;
                }
                let before_comment = &source_text[line_start..c.pos as usize];
                if !before_comment.trim().is_empty() {
                    let text = c.get_text(source_text);
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
            if c.pos >= stmt_end {
                break;
            }
        }
        None
    }

    /// Transform a namespace declaration to IR
    ///
    /// Returns `Some(IRNode::NamespaceIIFE { ... })` for valid namespaces,
    /// or `None` for ambient namespaces (declare namespace) or invalid nodes.
    ///
    /// # Arguments
    ///
    /// * `ns_idx` - NodeIndex of the namespace declaration
    ///
    /// # Returns
    ///
    /// `Option<IRNode>` - The transformed namespace as an IR node, or None if skipped
    pub fn transform_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        self.transform_namespace_with_flags(ns_idx, false, true)
    }

    /// Transform a namespace declaration with explicit control over var declaration
    pub fn transform_namespace_with_var_flag(
        &self,
        ns_idx: NodeIndex,
        should_declare_var: bool,
    ) -> Option<IRNode> {
        self.transform_namespace_with_flags(ns_idx, false, should_declare_var)
    }

    /// Transform a namespace declaration that is known to be exported
    ///
    /// Use this when the namespace is wrapped in an EXPORT_DECLARATION.
    pub fn transform_exported_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        self.transform_namespace_with_flags(ns_idx, true, true)
    }

    /// Transform a namespace declaration with explicit export and var flags
    fn transform_namespace_with_flags(
        &self,
        ns_idx: NodeIndex,
        force_exported: bool,
        should_declare_var: bool,
    ) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient namespaces (declare namespace)
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts for qualified names (A.B.C)
        // The parser creates nested MODULE_DECLARATION nodes for qualified names:
        // MODULE_DECLARATION "A" -> body: MODULE_DECLARATION "B" -> body: MODULE_DECLARATION "C" -> body: MODULE_BLOCK
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        // Check if exported from modifiers OR if forced (when wrapped in EXPORT_DECLARATION)
        let is_exported = force_exported || has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform the innermost body - use the last name part for member exports
        let mut body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        // A namespace is instantiated if it has any value declarations
        // (variables, functions, classes, enums, sub-namespaces),
        // even if the body produces no IR output (e.g., uninitialized exports).
        // Comments alone don't make a namespace instantiated.
        let has_code = body.iter().any(|n| !is_comment_node(n));
        if !has_code && !self.has_value_declarations(innermost_body) {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        // Root name is the first part
        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
            should_declare_var,
            parent_name: None,
            param_name,
            skip_sequence_indent: false,
        })
    }

    /// Check if a namespace body contains any value declarations
    fn has_value_declarations(&self, body_idx: NodeIndex) -> bool {
        body_has_value_declarations(self.arena, body_idx)
    }

    fn declaration_keyword_from_var_declarations(&self, declarations: &NodeList) -> &'static str {
        declaration_keyword_from_var_declarations(self.arena, declarations)
    }

    fn namespace_member_ast_ref_if_non_empty(&self, member_idx: NodeIndex) -> Option<IRNode> {
        if let Some(source_text) = self.source_text
            && let Some(member_node) = self.arena.get(member_idx)
        {
            let start = member_node.pos as usize;
            let end = (member_node.end as usize).min(source_text.len());
            if start < end {
                let raw = &source_text[start..end];
                if raw.trim().is_empty() {
                    return None;
                }
            }
        }

        Some(IRNode::ASTRef(member_idx))
    }

    /// Flatten a module name into parts (handles both identifiers and qualified names)
    ///
    /// For qualified names like `A.B.C` (parsed as nested MODULE_DECLARATIONs), returns `["A", "B", "C"]`.
    /// For simple identifiers like `foo`, returns `["foo"]`.
    ///
    /// Note: The parser creates nested MODULE_DECLARATION nodes for qualified namespace names,
    /// where each level has a single identifier name and the body points to the next level.
    pub fn flatten_module_name(&self, name_idx: NodeIndex) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        self.collect_name_parts(name_idx, &mut parts);
        if parts.is_empty() { None } else { Some(parts) }
    }

    /// Recursively collect name parts from qualified names
    ///
    /// Handles both:
    /// 1. QUALIFIED_NAME nodes (left.right structure)
    /// 2. Simple identifier nodes
    fn collect_name_parts(&self, idx: NodeIndex, parts: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // QualifiedName has left and right - recurse into both
            if let Some(qn_data) = self.arena.qualified_names.get(node.data_index as usize) {
                self.collect_name_parts(qn_data.left, parts);
                self.collect_name_parts(qn_data.right, parts);
            }
        } else if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
        {
            parts.push(ident.escaped_text.clone());
        }
    }

    /// Collect all name parts by walking through nested MODULE_DECLARATION chain
    ///
    /// For `namespace A.B.C {}`, the parser creates:
    /// MODULE_DECLARATION "A" -> body: MODULE_DECLARATION "B" -> body: MODULE_DECLARATION "C" -> body: MODULE_BLOCK
    ///
    /// This method walks through all levels and returns (["A", "B", "C"], innermost_body_idx)
    fn collect_all_namespace_parts(&self, ns_idx: NodeIndex) -> Option<(Vec<String>, NodeIndex)> {
        let mut parts = Vec::new();
        let mut current_idx = ns_idx;

        loop {
            let node = self.arena.get(current_idx)?;
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                // We've reached a non-namespace node (likely MODULE_BLOCK)
                break;
            }

            let ns_data = self.arena.get_module(node)?;

            // Get the name of this level
            let name_node = self.arena.get(ns_data.name)?;
            if let Some(ident) = self.arena.get_identifier(name_node) {
                parts.push(ident.escaped_text.clone());
            }

            // Check if body is another MODULE_DECLARATION (nested namespace) or MODULE_BLOCK
            let body_node = self.arena.get(ns_data.body)?;
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Continue walking nested declarations
                current_idx = ns_data.body;
            } else {
                // We've reached the innermost body (MODULE_BLOCK)
                return Some((parts, ns_data.body));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some((parts, current_idx))
        }
    }

    /// Transform namespace body into IR nodes
    fn transform_namespace_body(&self, body_idx: NodeIndex, name_parts: &[String]) -> Vec<IRNode> {
        let mut result = Vec::new();
        let runtime_exported_vars = collect_runtime_exported_var_names(self.arena, body_idx);

        // The innermost namespace name (last part) is used for member exports
        let ns_name = name_parts.last().map_or("", |s| s.as_str());

        let Some(body_node) = self.arena.get(body_idx) else {
            return result;
        };

        // Track names declared by classes, functions, enums so that subsequent
        // namespace declarations merging with them don't re-emit `var`.
        let mut declared_names = std::collections::HashSet::new();

        // First pass: collect declared names from classes, functions, enums
        if let Some(block_data) = self.arena.get_module_block(body_node)
            && let Some(stmts) = block_data.statements.as_ref()
        {
            for &stmt_idx in &stmts.nodes {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    match stmt_node.kind {
                        k if k == syntax_kind_ext::CLASS_DECLARATION => {
                            if let Some(class_data) = self.arena.get_class(stmt_node)
                                && let Some(name) = get_identifier_text(self.arena, class_data.name)
                            {
                                declared_names.insert(name);
                            }
                        }
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            if let Some(func_data) = self.arena.get_function(stmt_node)
                                && let Some(name) = get_identifier_text(self.arena, func_data.name)
                            {
                                declared_names.insert(name);
                            }
                        }
                        k if k == syntax_kind_ext::ENUM_DECLARATION => {
                            if let Some(enum_data) = self.arena.get_enum(stmt_node)
                                && let Some(name) = get_identifier_text(self.arena, enum_data.name)
                            {
                                declared_names.insert(name);
                            }
                        }
                        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                            if let Some(export_data) = self.arena.get_export_decl(stmt_node)
                                && let Some(inner) = self.arena.get(export_data.export_clause)
                            {
                                match inner.kind {
                                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                                        if let Some(class_data) = self.arena.get_class(inner)
                                            && let Some(name) =
                                                get_identifier_text(self.arena, class_data.name)
                                        {
                                            declared_names.insert(name);
                                        }
                                    }
                                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                                        if let Some(func_data) = self.arena.get_function(inner)
                                            && let Some(name) =
                                                get_identifier_text(self.arena, func_data.name)
                                        {
                                            declared_names.insert(name);
                                        }
                                    }
                                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                                        if let Some(enum_data) = self.arena.get_enum(inner)
                                            && let Some(name) =
                                                get_identifier_text(self.arena, enum_data.name)
                                        {
                                            declared_names.insert(name);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Find the position of the closing '}' of the module block.
        // The last statement's node.end may extend into this brace, so we
        // constrain ASTRef nodes to not include it.
        let body_close_pos = if let Some(text) = self.source_text {
            let mut pos = body_node.end as usize;
            while pos > body_node.pos as usize {
                pos -= 1;
                if text.as_bytes().get(pos) == Some(&b'}') {
                    break;
                }
            }
            pos as u32
        } else {
            body_node.end.saturating_sub(1)
        };

        // Check if it's a module block
        if let Some(block_data) = self.arena.get_module_block(body_node)
            && let Some(stmts) = block_data.statements.as_ref()
        {
            // Track cursor for comment extraction between statements.
            // Start after the opening brace of the module block.
            let mut prev_end = body_node.pos + 1; // skip past '{'
            let mut prev_stmt_pos = body_node.pos + 1;

            for &stmt_idx in &stmts.nodes {
                let stmt_node = match self.arena.get(stmt_idx) {
                    Some(n) => n,
                    None => continue,
                };

                // Some statements have trailing trivia that includes standalone comments
                // before the next declaration. Capture those comments here so they can
                // be emitted immediately after the current statement.
                let code_end = self.find_code_end_of_erased_stmt(stmt_node.pos, stmt_node.end);
                let trailing_standalone =
                    self.extract_standalone_comments_in_range(code_end, stmt_node.end);

                // Extract leading comments between previous end and this statement.
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                if prev_end <= actual_start {
                    let leading_comments = self.extract_comments_in_range(prev_end, actual_start);
                    for c in leading_comments {
                        result.push(c);
                    }
                } else if prev_end == stmt_node.pos && prev_stmt_pos <= actual_start {
                    // Parser trivia-skipping can move `stmt_node.end` to the next statement token,
                    // which can skip standalone comments on blank lines. Recover those comments
                    // by probing from the previous statement start as a fallback.
                    let fallback_comments =
                        self.extract_comments_in_range(prev_stmt_pos, actual_start);
                    for c in fallback_comments {
                        result.push(c);
                    }
                }

                let ir = self.transform_namespace_member_with_declared(
                    ns_name,
                    stmt_idx,
                    &declared_names,
                );

                if let Some(ir) = ir {
                    // Constrain ASTRef nodes so their source text doesn't extend
                    // into the module block's closing brace.
                    let ir = if let IRNode::ASTRef(idx) = ir {
                        IRNode::ASTRefRange(idx, body_close_pos)
                    } else {
                        ir
                    };

                    // Check for trailing comment on the same line as this statement.
                    // Skip namespace/class declarations since their sub-emitters handle
                    // internal comments.
                    let export_clause_kind =
                        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            self.arena
                                .get_export_decl(stmt_node)
                                .and_then(|d| self.arena.get(d.export_clause))
                                .map(|n| n.kind)
                        } else {
                            None
                        };
                    let skip = is_namespace_like(self.arena, stmt_node)
                        || stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || matches!(export_clause_kind, Some(k) if k == syntax_kind_ext::CLASS_DECLARATION || k == syntax_kind_ext::MODULE_DECLARATION);
                    let trailing =
                        self.extract_trailing_comment_in_stmt(stmt_node.pos, stmt_node.end);
                    let mut ir = ir;
                    let mut trailing_attached_in_sequence = false;
                    // For exported function declarations inside namespaces, attach trailing
                    // comments to the function declaration, not the namespace export assignment.
                    if let IRNode::Sequence(items) = &mut ir
                        && let Some(comment_text) = trailing.clone()
                        && items.len() > 1
                        && matches!(items.first(), Some(IRNode::FunctionDecl { .. }))
                    {
                        items.insert(1, IRNode::TrailingComment(comment_text));
                        trailing_attached_in_sequence = true;
                    }
                    result.push(ir);
                    if !skip
                        && !trailing_attached_in_sequence
                        && let Some(comment_text) = trailing
                    {
                        result.push(IRNode::TrailingComment(comment_text));
                    }
                } else {
                    // Erased statement (interface/type alias).
                    // (Standalone trailing comments are now emitted above for all
                    // statement kinds.)
                }

                for c in trailing_standalone {
                    result.push(c);
                }

                prev_end = stmt_node.end;
                prev_stmt_pos = stmt_node.pos;
            }

            // Extract standalone comments after the last statement but before the closing brace.
            // Since node.end includes trailing trivia, these are comments NOT part of any
            // statement's trivia — they appear on their own lines before `}`.
            if let Some(last_stmt) = stmts.nodes.last()
                && let Some(last_node) = self.arena.get(*last_stmt)
            {
                let standalone_comments =
                    self.extract_comments_in_range(last_node.end, body_close_pos);
                for c in standalone_comments {
                    result.push(c);
                }
            }
        }

        if !runtime_exported_vars.is_empty() {
            for node in &mut result {
                rewrite_exported_var_refs(node, ns_name, &runtime_exported_vars);
            }
        }

        result
    }

    /// Transform a namespace member, considering already-declared names for `should_declare_var`
    fn transform_namespace_member_with_declared(
        &self,
        ns_name: &str,
        member_idx: NodeIndex,
        declared_names: &std::collections::HashSet<String>,
    ) -> Option<IRNode> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                // Check if a class/function/enum already declared this name
                let ns_data = self.arena.get_module(member_node)?;
                let name = get_identifier_text(self.arena, ns_data.name)?;
                let should_declare_var = !declared_names.contains(&name);
                self.transform_nested_namespace_with_var(ns_name, member_idx, should_declare_var)
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    if let Some(inner) = self.arena.get(export_data.export_clause)
                        && inner.kind == syntax_kind_ext::MODULE_DECLARATION
                    {
                        let ns_data = self.arena.get_module(inner)?;
                        let name = get_identifier_text(self.arena, ns_data.name)?;
                        let should_declare_var = !declared_names.contains(&name);
                        return self.transform_nested_namespace_exported_with_var(
                            ns_name,
                            export_data.export_clause,
                            should_declare_var,
                        );
                    }
                    self.transform_namespace_member_exported(ns_name, export_data.export_clause)
                } else {
                    None
                }
            }
            _ => self.transform_namespace_member(ns_name, member_idx),
        }
    }

    /// Transform a namespace member to IR
    fn transform_namespace_member(&self, ns_name: &str, member_idx: NodeIndex) -> Option<IRNode> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Handle export declarations by extracting the inner declaration
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    self.transform_namespace_member_exported(ns_name, export_data.export_clause)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.transform_import_equals_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => None,
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => None,
            _ => self.namespace_member_ast_ref_if_non_empty(member_idx),
        }
    }

    /// Transform an exported namespace member
    fn transform_namespace_member_exported(
        &self,
        ns_name: &str,
        decl_idx: NodeIndex,
    ) -> Option<IRNode> {
        let decl_node = self.arena.get(decl_idx)?;

        match decl_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.transform_import_equals_exported(ns_name, decl_idx)
            }
            _ => None,
        }
    }

    fn transform_import_equals_in_namespace(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        let alias = get_identifier_text(self.arena, import.import_clause)?;
        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);
        let is_exported = has_export_modifier(self.arena, &import.modifiers);

        if is_exported {
            Some(IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: alias,
                value: Box::new(target_expr),
            })
        } else {
            Some(IRNode::VarDecl {
                name: alias,
                initializer: Some(Box::new(target_expr)),
            })
        }
    }

    fn transform_import_equals_exported(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        let alias = get_identifier_text(self.arena, import.import_clause)?;

        // Skip export-import aliases that point to type-only namespaces.
        // These are emitted as no-op in TypeScript emit output.
        let should_emit_alias = if let Some(target_parts) =
            collect_qualified_name_parts(self.arena, import.module_specifier)
        {
            if let Some(body) = namespace_body_by_name(self.arena, &target_parts) {
                body_has_value_declarations(self.arena, body)
            } else {
                true
            }
        } else {
            true
        };

        if !should_emit_alias {
            return None;
        }

        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);

        Some(IRNode::NamespaceExport {
            namespace: ns_name.to_string(),
            name: alias,
            value: Box::new(target_expr),
        })
    }

    /// Transform a function in namespace
    fn transform_function_in_namespace(
        &self,
        ns_name: &str,
        func_idx: NodeIndex,
    ) -> Option<IRNode> {
        let func_data = self.arena.get_function_at(func_idx)?;

        // Skip declaration-only functions (no body)
        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let is_exported = has_export_modifier(self.arena, &func_data.modifiers);

        let body_source_range = self.arena.get(func_data.body).map(|n| (n.pos, n.end));

        // Convert function to IR (stripping type annotations)
        let func_decl = IRNode::FunctionDecl {
            name: func_name.clone(),
            parameters: convert_function_parameters(self.arena, &func_data.parameters),
            body: convert_function_body(self.arena, func_data.body),
            body_source_range,
        };

        if is_exported {
            Some(IRNode::Sequence(vec![
                func_decl,
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: func_name.clone(),
                    value: Box::new(IRNode::Identifier(func_name.clone())),
                },
            ]))
        } else {
            Some(func_decl)
        }
    }

    /// Transform an exported function
    fn transform_function_exported(&self, ns_name: &str, func_idx: NodeIndex) -> Option<IRNode> {
        let func_data = self.arena.get_function_at(func_idx)?;

        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let body_source_range = self.arena.get(func_data.body).map(|n| (n.pos, n.end));

        // Convert function to IR (stripping type annotations)
        let func_decl = IRNode::FunctionDecl {
            name: func_name.clone(),
            parameters: convert_function_parameters(self.arena, &func_data.parameters),
            body: convert_function_body(self.arena, func_data.body),
            body_source_range,
        };

        Some(IRNode::Sequence(vec![
            func_decl,
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: func_name.clone(),
                value: Box::new(IRNode::Identifier(func_name)),
            },
        ]))
    }

    /// Transform a class in namespace
    fn transform_class_in_namespace(&self, ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_data = self.arena.get_class_at(class_idx)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        let is_exported = has_export_modifier(self.arena, &class_data.modifiers);

        // Transform the class to ES5 using the class transformer
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        let class_ir = class_transformer.transform_class_to_ir(class_idx)?;

        if is_exported {
            Some(IRNode::Sequence(vec![
                class_ir,
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: class_name.clone(),
                    value: Box::new(IRNode::Identifier(class_name)),
                },
            ]))
        } else {
            Some(class_ir)
        }
    }

    /// Transform an exported class
    fn transform_class_exported(&self, ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_data = self.arena.get_class_at(class_idx)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;

        // Transform the class to ES5 using the class transformer
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        let class_ir = class_transformer.transform_class_to_ir(class_idx)?;

        Some(IRNode::Sequence(vec![
            class_ir,
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: class_name.clone(),
                value: Box::new(IRNode::Identifier(class_name)),
            },
        ]))
    }

    /// Transform a variable statement in namespace
    fn transform_variable_in_namespace(&self, ns_name: &str, var_idx: NodeIndex) -> Option<IRNode> {
        let var_data = self.arena.get_variable_at(var_idx)?;

        let is_exported = has_export_modifier(self.arena, &var_data.modifiers);

        if is_exported {
            // For exported variables, emit directly as namespace property assignments:
            // `Namespace.X = initializer;` instead of `var X = initializer; Namespace.X = X;`
            Some(IRNode::Sequence(convert_exported_variable_declarations(
                self.arena,
                &var_data.declarations,
                ns_name,
            )))
        } else {
            let empty_decl_keyword =
                self.declaration_keyword_from_var_declarations(&var_data.declarations);
            let decls = convert_variable_declarations(
                self.arena,
                &var_data.declarations,
                empty_decl_keyword,
            );
            Some(IRNode::Sequence(decls))
        }
    }

    /// Transform an exported variable
    fn transform_variable_exported(&self, ns_name: &str, var_idx: NodeIndex) -> Option<IRNode> {
        let var_data = self.arena.get_variable_at(var_idx)?;

        // For exported variables, emit directly as namespace property assignments:
        // `Namespace.X = initializer;` instead of `var X = initializer; Namespace.X = X;`
        Some(IRNode::Sequence(convert_exported_variable_declarations(
            self.arena,
            &var_data.declarations,
            ns_name,
        )))
    }

    /// Transform an enum in namespace
    fn transform_enum_in_namespace(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        let is_exported = has_export_modifier(self.arena, &enum_data.modifiers);

        let enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;
        let mut result = vec![enum_ir];

        if is_exported {
            result.push(IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            });
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an exported enum
    fn transform_enum_exported(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        Some(IRNode::Sequence(vec![
            transform_enum_to_ir(self.arena, enum_idx)?,
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            },
        ]))
    }

    /// Transform a nested namespace
    fn transform_nested_namespace(&self, parent_ns: &str, ns_idx: NodeIndex) -> Option<IRNode> {
        self.transform_nested_namespace_with_var(parent_ns, ns_idx, true)
    }

    fn transform_nested_namespace_with_var(
        &self,
        parent_ns: &str,
        ns_idx: NodeIndex,
        should_declare_var: bool,
    ) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        let name_parts = self.flatten_module_name(ns_data.name)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform body
        let mut body = self.transform_namespace_body(ns_data.body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        if !body.iter().any(|n| !is_comment_node(n)) && !self.has_value_declarations(ns_data.body) {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
            should_declare_var,
            parent_name: is_exported.then(|| parent_ns.to_string()),
            param_name,
            skip_sequence_indent: true, // Nested namespace IIFEs need to skip indent when in sequence
        })
    }

    /// Transform an exported nested namespace
    fn transform_nested_namespace_exported(
        &self,
        parent_ns: &str,
        ns_idx: NodeIndex,
    ) -> Option<IRNode> {
        self.transform_nested_namespace_exported_with_var(parent_ns, ns_idx, true)
    }

    fn transform_nested_namespace_exported_with_var(
        &self,
        parent_ns: &str,
        ns_idx: NodeIndex,
        should_declare_var: bool,
    ) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        let name_parts = self.flatten_module_name(ns_data.name)?;
        if name_parts.is_empty() {
            return None;
        }

        // Always exported since this is from an export declaration
        let is_exported = true;

        // Transform body
        let mut body = self.transform_namespace_body(ns_data.body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        if !body.iter().any(|n| !is_comment_node(n)) && !self.has_value_declarations(ns_data.body) {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
            should_declare_var,
            parent_name: Some(parent_ns.to_string()),
            param_name,
            skip_sequence_indent: true, // Nested namespace IIFEs need to skip indent when in sequence
        })
    }
}

// =============================================================================
// NamespaceTransformContext - Legacy context (for backward compatibility)
// =============================================================================

/// Context for namespace transformation (legacy, use NamespaceES5Transformer instead)
pub struct NamespaceTransformContext<'a> {
    arena: &'a NodeArena,
    is_commonjs: bool,
}

impl<'a> NamespaceTransformContext<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            is_commonjs: false,
        }
    }

    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        Self { arena, is_commonjs }
    }

    /// Transform a namespace declaration to IR
    pub fn transform_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts (handles nested MODULE_DECLARATIONs for A.B.C)
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform body
        let mut body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        // A namespace is instantiated if it has any value declarations
        // (variables, functions, classes, enums, sub-namespaces),
        // even if the body produces no IR output (e.g., uninitialized exports).
        if !body.iter().any(|n| !is_comment_node(n)) && !self.has_value_declarations(innermost_body)
        {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: self.is_commonjs,
            should_declare_var: true,
            parent_name: None,
            param_name,
            skip_sequence_indent: false,
        })
    }

    /// Check if a namespace body contains any value declarations
    fn has_value_declarations(&self, body_idx: NodeIndex) -> bool {
        body_has_value_declarations(self.arena, body_idx)
    }

    fn declaration_keyword_from_var_declarations(&self, declarations: &NodeList) -> &'static str {
        declaration_keyword_from_var_declarations(self.arena, declarations)
    }

    fn namespace_member_ast_ref_if_non_empty(&self, member_idx: NodeIndex) -> Option<IRNode> {
        Some(IRNode::ASTRef(member_idx))
    }

    /// Collect all name parts by walking through nested MODULE_DECLARATION chain
    fn collect_all_namespace_parts(&self, ns_idx: NodeIndex) -> Option<(Vec<String>, NodeIndex)> {
        let mut parts = Vec::new();
        let mut current_idx = ns_idx;

        loop {
            let node = self.arena.get(current_idx)?;
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                break;
            }

            let ns_data = self.arena.get_module(node)?;

            // Get the name of this level
            let name_node = self.arena.get(ns_data.name)?;
            if let Some(ident) = self.arena.get_identifier(name_node) {
                parts.push(ident.escaped_text.clone());
            }

            // Check if body is another MODULE_DECLARATION (nested namespace) or MODULE_BLOCK
            let body_node = self.arena.get(ns_data.body)?;
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                current_idx = ns_data.body;
            } else {
                return Some((parts, ns_data.body));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some((parts, current_idx))
        }
    }

    /// Transform namespace body
    fn transform_namespace_body(&self, body_idx: NodeIndex, name_parts: &[String]) -> Vec<IRNode> {
        let mut result = Vec::new();
        let runtime_exported_vars = collect_runtime_exported_var_names(self.arena, body_idx);
        let ns_name = name_parts.last().map_or("", |s| s.as_str());

        let Some(body_node) = self.arena.get(body_idx) else {
            return result;
        };

        // Check if it's a module block
        if let Some(block_data) = self.arena.get_module_block(body_node)
            && let Some(stmts) = block_data.statements.as_ref()
        {
            for &stmt_idx in &stmts.nodes {
                if let Some(ir) = self.transform_namespace_member(ns_name, stmt_idx) {
                    result.push(ir);
                }
            }
        }

        if !runtime_exported_vars.is_empty() {
            for node in &mut result {
                rewrite_exported_var_refs(node, ns_name, &runtime_exported_vars);
            }
        }

        result
    }

    /// Transform a namespace member
    fn transform_namespace_member(&self, ns_name: &str, member_idx: NodeIndex) -> Option<IRNode> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Handle export declarations by extracting the inner declaration
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    self.transform_namespace_member_exported(ns_name, export_data.export_clause)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => None,
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => None,
            _ => self.namespace_member_ast_ref_if_non_empty(member_idx),
        }
    }

    /// Transform an exported namespace member
    pub fn transform_namespace_member_exported(
        &self,
        ns_name: &str,
        decl_idx: NodeIndex,
    ) -> Option<IRNode> {
        let decl_node = self.arena.get(decl_idx)?;

        match decl_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace_exported(ns_name, decl_idx)
            }
            _ => None,
        }
    }

    /// Transform a function in namespace context
    pub fn transform_function_in_namespace(
        &self,
        ns_name: &str,
        func_idx: NodeIndex,
    ) -> Option<IRNode> {
        let func_data = self.arena.get_function_at(func_idx)?;

        // Skip declaration-only functions
        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let is_exported = has_export_modifier(self.arena, &func_data.modifiers);

        let body_source_range = self.arena.get(func_data.body).map(|n| (n.pos, n.end));

        // Convert function to IR (stripping type annotations)
        let func_decl = IRNode::FunctionDecl {
            name: func_name.clone(),
            parameters: convert_function_parameters(self.arena, &func_data.parameters),
            body: convert_function_body(self.arena, func_data.body),
            body_source_range,
        };

        if is_exported {
            Some(IRNode::Sequence(vec![
                func_decl,
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: func_name.clone(),
                    value: Box::new(IRNode::Identifier(func_name.clone())),
                },
            ]))
        } else {
            Some(func_decl)
        }
    }

    /// Transform an exported function in namespace
    fn transform_function_in_namespace_exported(
        &self,
        ns_name: &str,
        func_idx: NodeIndex,
    ) -> Option<IRNode> {
        let func_data = self.arena.get_function_at(func_idx)?;

        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let body_source_range = self.arena.get(func_data.body).map(|n| (n.pos, n.end));

        // Convert function to IR (stripping type annotations)
        let func_decl = IRNode::FunctionDecl {
            name: func_name.clone(),
            parameters: convert_function_parameters(self.arena, &func_data.parameters),
            body: convert_function_body(self.arena, func_data.body),
            body_source_range,
        };

        Some(IRNode::Sequence(vec![
            func_decl,
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: func_name.clone(),
                value: Box::new(IRNode::Identifier(func_name)),
            },
        ]))
    }

    /// Transform a class in namespace context
    fn transform_class_in_namespace(&self, ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_data = self.arena.get_class_at(class_idx)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        let is_exported = has_export_modifier(self.arena, &class_data.modifiers);

        // Transform the class to ES5 using the class transformer
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        let class_ir = class_transformer.transform_class_to_ir(class_idx)?;

        if is_exported {
            Some(IRNode::Sequence(vec![
                class_ir,
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: class_name.clone(),
                    value: Box::new(IRNode::Identifier(class_name)),
                },
            ]))
        } else {
            Some(class_ir)
        }
    }

    /// Transform an exported class in namespace
    fn transform_class_in_namespace_exported(
        &self,
        ns_name: &str,
        class_idx: NodeIndex,
    ) -> Option<IRNode> {
        let class_data = self.arena.get_class_at(class_idx)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;

        // Transform the class to ES5 using the class transformer
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        let class_ir = class_transformer.transform_class_to_ir(class_idx)?;

        Some(IRNode::Sequence(vec![
            class_ir,
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: class_name.clone(),
                value: Box::new(IRNode::Identifier(class_name)),
            },
        ]))
    }

    /// Transform a variable statement in namespace
    pub fn transform_variable_in_namespace(
        &self,
        ns_name: &str,
        var_idx: NodeIndex,
    ) -> Option<IRNode> {
        let var_data = self.arena.get_variable_at(var_idx)?;

        let is_exported = has_export_modifier(self.arena, &var_data.modifiers);
        if is_exported {
            Some(IRNode::Sequence(convert_exported_variable_declarations(
                self.arena,
                &var_data.declarations,
                ns_name,
            )))
        } else {
            let empty_decl_keyword =
                self.declaration_keyword_from_var_declarations(&var_data.declarations);
            Some(IRNode::Sequence(convert_variable_declarations(
                self.arena,
                &var_data.declarations,
                empty_decl_keyword,
            )))
        }
    }

    /// Transform an exported variable statement in namespace
    fn transform_variable_in_namespace_exported(
        &self,
        ns_name: &str,
        var_idx: NodeIndex,
    ) -> Option<IRNode> {
        let var_data = self.arena.get_variable_at(var_idx)?;

        Some(IRNode::Sequence(convert_exported_variable_declarations(
            self.arena,
            &var_data.declarations,
            ns_name,
        )))
    }

    /// Transform an enum in namespace
    fn transform_enum_in_namespace(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        let is_exported = has_export_modifier(self.arena, &enum_data.modifiers);

        let enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;
        let mut result = vec![enum_ir];

        if is_exported {
            result.push(IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            });
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an exported enum in namespace
    fn transform_enum_in_namespace_exported(
        &self,
        ns_name: &str,
        enum_idx: NodeIndex,
    ) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        Some(IRNode::Sequence(vec![
            transform_enum_to_ir(self.arena, enum_idx)?,
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            },
        ]))
    }

    /// Transform a nested namespace
    pub fn transform_nested_namespace(&self, parent_ns: &str, ns_idx: NodeIndex) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts (handles nested MODULE_DECLARATIONs)
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform body
        let mut body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        // A namespace is instantiated if it has any value declarations
        // (variables, functions, classes, enums, sub-namespaces),
        // even if the body produces no IR output (e.g., uninitialized exports).
        if !body.iter().any(|n| !is_comment_node(n)) && !self.has_value_declarations(innermost_body)
        {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: self.is_commonjs,
            should_declare_var: true,
            parent_name: is_exported.then(|| parent_ns.to_string()),
            param_name,
            skip_sequence_indent: true,
        })
    }

    /// Transform an exported nested namespace
    fn transform_nested_namespace_exported(
        &self,
        parent_ns: &str,
        ns_idx: NodeIndex,
    ) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts (handles nested MODULE_DECLARATIONs)
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = true; // Always exported

        // Transform body
        let mut body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        // A namespace is instantiated if it has any value declarations
        // (variables, functions, classes, enums, sub-namespaces),
        // even if the body produces no IR output (e.g., uninitialized exports).
        if !body.iter().any(|n| !is_comment_node(n)) && !self.has_value_declarations(innermost_body)
        {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: self.is_commonjs,
            should_declare_var: true,
            parent_name: Some(parent_ns.to_string()),
            param_name,
            skip_sequence_indent: true,
        })
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if a namespace body (MODULE_BLOCK) contains any value declarations.
/// Value declarations are: variables, functions, classes, enums, sub-namespaces.
/// Type-only declarations (interfaces, type aliases) don't count.
fn body_has_value_declarations(arena: &NodeArena, body_idx: NodeIndex) -> bool {
    let Some(body_node) = arena.get(body_idx) else {
        return false;
    };

    let Some(block_data) = arena.get_module_block(body_node) else {
        return false;
    };

    let Some(stmts) = block_data.statements.as_ref() else {
        return false;
    };

    for &stmt_idx in &stmts.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION =>
            {
                return true;
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                // Recursively check if nested namespace is itself instantiated
                if let Some(ns_data) = arena.get_module(stmt_node)
                    && body_has_value_declarations(arena, ns_data.body)
                {
                    return true;
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Check if the exported declaration is a value declaration
                if let Some(export_data) = arena.get_export_decl(stmt_node)
                    && let Some(inner_node) = arena.get(export_data.export_clause)
                {
                    match inner_node.kind {
                        k if k == syntax_kind_ext::VARIABLE_STATEMENT
                            || k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::ENUM_DECLARATION =>
                        {
                            return true;
                        }
                        k if k == syntax_kind_ext::MODULE_DECLARATION => {
                            if let Some(ns_data) = arena.get_module(inner_node)
                                && body_has_value_declarations(arena, ns_data.body)
                            {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    false
}

/// Check if an IR node is a comment (standalone or trailing).
/// Used to determine if a namespace body has only comments and no actual code.
fn is_comment_node(node: &IRNode) -> bool {
    matches!(node, IRNode::Raw(s) if s.starts_with("//") || s.starts_with("/*"))
        || matches!(node, IRNode::TrailingComment(_))
}

/// Check if a node is a namespace-like declaration (MODULE_DECLARATION or
/// EXPORT_DECLARATION wrapping MODULE_DECLARATION). These have block bodies
/// whose internal comments are handled by the sub-emitter.
fn is_namespace_like(arena: &NodeArena, node: &tsz_parser::parser::node::Node) -> bool {
    if node.kind == syntax_kind_ext::MODULE_DECLARATION {
        return true;
    }
    if node.kind == syntax_kind_ext::EXPORT_DECLARATION
        && let Some(export_data) = arena.get_export_decl(node)
        && let Some(inner) = arena.get(export_data.export_clause)
    {
        return inner.kind == syntax_kind_ext::MODULE_DECLARATION;
    }
    false
}

fn get_identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else {
        None
    }
}

fn has_modifier(arena: &NodeArena, modifiers: &Option<NodeList>, kind: u16) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx)
                && mod_node.kind == kind
            {
                return true;
            }
        }
    }
    false
}

fn has_declare_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::DeclareKeyword as u16)
}

fn has_export_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::ExportKeyword as u16)
}

/// Convert function parameters to IR parameters (without type annotations)
fn convert_function_parameters(arena: &NodeArena, params: &NodeList) -> Vec<IRParam> {
    params
        .nodes
        .iter()
        .filter_map(|&p| {
            let param = arena.get_parameter_at(p)?;
            let name = get_identifier_text(arena, param.name)?;
            let rest = param.dot_dot_dot_token;
            // Convert default value if present
            let default_value = (!param.initializer.is_none())
                .then(|| Box::new(AstToIr::new(arena).convert_expression(param.initializer)));
            Some(IRParam {
                name,
                rest,
                default_value,
            })
        })
        .collect()
}

/// Convert function body to IR statements (without type annotations)
fn convert_function_body(arena: &NodeArena, body_idx: NodeIndex) -> Vec<IRNode> {
    let Some(body_node) = arena.get(body_idx) else {
        return vec![];
    };

    // Handle both Block and syntax_kind_ext::BLOCK
    if body_node.kind == syntax_kind_ext::BLOCK
        && let Some(block) = arena.get_block(body_node)
    {
        return block
            .statements
            .nodes
            .iter()
            .map(|&s| AstToIr::new(arena).convert_statement(s))
            .collect();
    }

    // Fallback for unsupported body types
    vec![]
}

fn collect_runtime_exported_var_names(
    arena: &NodeArena,
    body_idx: NodeIndex,
) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();

    let Some(body_node) = arena.get(body_idx) else {
        return names;
    };
    let Some(block_data) = arena.get_module_block(body_node) else {
        return names;
    };
    let Some(stmts) = block_data.statements.as_ref() else {
        return names;
    };

    for &stmt_idx in &stmts.nodes {
        collect_runtime_exported_var_names_in_stmt(arena, stmt_idx, &mut names);
    }

    names
}

fn collect_runtime_exported_var_names_in_stmt(
    arena: &NodeArena,
    stmt_idx: NodeIndex,
    names: &mut std::collections::HashSet<String>,
) {
    let Some(stmt_node) = arena.get(stmt_idx) else {
        return;
    };

    let collect_from_var_statement =
        |node: &Node, names: &mut std::collections::HashSet<String>| {
            if let Some(var_data) = arena.get_variable(node) {
                for &decl_list_idx in &var_data.declarations.nodes {
                    if let Some(decl_list_node) = arena.get(decl_list_idx)
                        && let Some(decl_list) = arena.get_variable(decl_list_node)
                    {
                        for &decl_idx in &decl_list.declarations.nodes {
                            if let Some(decl_node) = arena.get(decl_idx)
                                && let Some(decl) = arena.get_variable_declaration(decl_node)
                                && let Some(name_node) = arena.get(decl.name)
                            {
                                let Some(name) = arena
                                    .get_identifier(name_node)
                                    .map(|id| id.escaped_text.clone())
                                else {
                                    continue;
                                };
                                if decl.initializer.is_none() {
                                    continue;
                                }
                                names.insert(name);
                            }
                        }
                    }
                }
            }
        };

    match stmt_node.kind {
        kind if kind == syntax_kind_ext::VARIABLE_STATEMENT => {
            collect_from_var_statement(stmt_node, names);
        }
        kind if kind == syntax_kind_ext::EXPORT_DECLARATION => {
            if let Some(export_data) = arena.get_export_decl(stmt_node)
                && let Some(inner_node) = arena.get(export_data.export_clause)
                && inner_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            {
                collect_from_var_statement(inner_node, names);
            }
        }
        _ => {}
    }
}

/// Convert exported variable declarations directly to namespace property assignments.
/// Instead of `var X = init; NS.X = X;`, emits `NS.X = init;` (matching tsc).
fn convert_exported_variable_declarations(
    arena: &NodeArena,
    declarations: &NodeList,
    ns_name: &str,
) -> Vec<IRNode> {
    let mut result = Vec::new();
    let mut assignment_targets: Vec<(String, IRNode)> = Vec::new();

    for &decl_list_idx in &declarations.nodes {
        if let Some(decl_list_node) = arena.get(decl_list_idx)
            && let Some(decl_list) = arena.get_variable(decl_list_node)
        {
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl_node) = arena.get(decl_idx)
                    && let Some(decl) = arena.get_variable_declaration(decl_node)
                    && let Some(name) = get_identifier_text(arena, decl.name)
                    && !decl.initializer.is_none()
                {
                    let value = AstToIr::new(arena).convert_expression(decl.initializer);
                    assignment_targets.push((name, value));
                }
                // No initializer: tsc omits the assignment entirely in namespaces
            }
        }
    }

    if assignment_targets.is_empty() {
        return result;
    }

    if assignment_targets.len() == 1 {
        let (name, value) = assignment_targets.remove(0);
        return vec![IRNode::NamespaceExport {
            namespace: ns_name.to_string(),
            name,
            value: Box::new(value),
        }];
    }

    let parts: Vec<String> = assignment_targets
        .into_iter()
        .map(|(name, value)| format!("{}.{name} = {}", ns_name, IRPrinter::emit_to_string(&value)))
        .collect();
    result.push(IRNode::Raw(format!("{};", parts.join(", "))));

    result
}

/// Convert variable declarations to proper IR (VarDecl nodes)
fn convert_variable_declarations(
    arena: &NodeArena,
    declarations: &NodeList,
    empty_decl_keyword: &str,
) -> Vec<IRNode> {
    let mut result = Vec::new();

    for &decl_list_idx in &declarations.nodes {
        let decl_list_node = arena.get(decl_list_idx);
        if let (Some(decl_list_node), Some(decl_list)) =
            (decl_list_node, arena.get_variable_at(decl_list_idx))
        {
            let mut emitted_any = false;
            let keyword = if decl_list_node.flags == 0 {
                empty_decl_keyword
            } else {
                declaration_keyword_from_flags(decl_list_node.flags)
            };
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl) = arena.get_variable_declaration_at(decl_idx)
                    && let Some(name) = get_identifier_text(arena, decl.name)
                {
                    // Use AstToIr for eager lowering of initializers
                    // This converts expressions to proper IR (NumericLiteral, CallExpr, etc.)
                    let initializer = (!decl.initializer.is_none()).then(|| {
                        Box::new(AstToIr::new(arena).convert_expression(decl.initializer))
                    });

                    result.push(IRNode::VarDecl { name, initializer });
                    emitted_any = true;
                }
            }

            if !emitted_any && decl_list.declarations.nodes.is_empty() {
                // Preserve declaration-shape recovery output such as `var ;` / `let;`.
                if keyword == "var" {
                    result.push(IRNode::Raw("var ;".to_string()));
                } else {
                    result.push(IRNode::Raw(format!("{keyword};")));
                }
            }
        }
    }

    result
}

fn declaration_keyword_from_flags(flags: u16) -> &'static str {
    if (flags & node_flags::LET as u16) != 0 {
        "let"
    } else {
        // TypeScript emits `const` declarations as `var` in emitted JS output.
        "var"
    }
}

fn declaration_keyword_from_var_declarations(
    arena: &NodeArena,
    declarations: &NodeList,
) -> &'static str {
    for &decl_list_idx in &declarations.nodes {
        let Some(decl_list_node) = arena.get(decl_list_idx) else {
            continue;
        };

        if arena.get_variable_at(decl_list_idx).is_some() {
            let keyword = declaration_keyword_from_flags(decl_list_node.flags);
            if keyword == "let" {
                return "let";
            }
        }
    }

    "var"
}

// =============================================================================
// Namespace IIFE parameter collision detection and renaming
// =============================================================================

/// Collect all member names declared in the namespace body IR.
/// These are names that would clash with the IIFE parameter if they match the namespace name.
fn collect_body_member_names(body: &[IRNode]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for node in body {
        collect_member_names_from_node(node, &mut names);
    }
    names
}

/// Recursively collect declared names from IR nodes
fn collect_member_names_from_node(node: &IRNode, names: &mut std::collections::HashSet<String>) {
    match node {
        IRNode::ES5ClassIIFE { name, .. } => {
            names.insert(name.clone());
        }
        IRNode::FunctionDecl { name, .. } => {
            names.insert(name.clone());
        }
        IRNode::VarDecl { name, .. } => {
            names.insert(name.clone());
        }
        IRNode::EnumIIFE { name, .. } => {
            names.insert(name.clone());
        }
        IRNode::Sequence(items) => {
            for item in items {
                collect_member_names_from_node(item, names);
            }
        }
        _ => {}
    }
}

/// Generate a unique parameter name by appending `_1`, `_2`, etc.
/// Ensures the generated name doesn't collide with any existing member name.
fn generate_unique_param_name(
    ns_name: &str,
    member_names: &std::collections::HashSet<String>,
) -> String {
    let mut suffix = 1;
    loop {
        let candidate = format!("{}_{}", ns_name, suffix);
        if !member_names.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Rename namespace references in body IR nodes.
/// Updates `NamespaceExport.namespace` and nested `NamespaceIIFE.parent_name`
/// from `old_name` to `new_name`.
fn rename_namespace_refs_in_body(body: &mut [IRNode], old_name: &str, new_name: &str) {
    for node in body.iter_mut() {
        rename_namespace_refs_in_node(node, old_name, new_name);
    }
}

/// Recursively rename namespace references in a single IR node
fn rename_namespace_refs_in_node(node: &mut IRNode, old_name: &str, new_name: &str) {
    match node {
        IRNode::NamespaceExport { namespace, .. } => {
            if namespace == old_name {
                *namespace = new_name.to_string();
            }
        }
        IRNode::NamespaceIIFE { parent_name, .. } => {
            if let Some(parent) = parent_name
                && parent == old_name
            {
                *parent = new_name.to_string();
            }
        }
        IRNode::Sequence(items) => {
            for item in items.iter_mut() {
                rename_namespace_refs_in_node(item, old_name, new_name);
            }
        }
        _ => {}
    }
}

/// Detect collision between namespace name and body member names,
/// and if found, rename the body's namespace references and return the new parameter name.
fn detect_and_apply_param_rename(body: &mut [IRNode], ns_name: &str) -> Option<String> {
    let member_names = collect_body_member_names(body);
    member_names.contains(ns_name).then(|| {
        let renamed = generate_unique_param_name(ns_name, &member_names);
        rename_namespace_refs_in_body(body, ns_name, &renamed);
        renamed
    })
}

fn rewrite_exported_var_refs(
    node: &mut IRNode,
    ns_name: &str,
    names: &std::collections::HashSet<String>,
) {
    match node {
        IRNode::Identifier(name) => {
            if names.contains(name) {
                let property = name.clone();
                *node = IRNode::PropertyAccess {
                    object: Box::new(IRNode::Identifier(ns_name.to_string())),
                    property,
                };
            }
        }
        IRNode::BinaryExpr { left, right, .. } => {
            rewrite_exported_var_refs(left, ns_name, names);
            rewrite_exported_var_refs(right, ns_name, names);
        }
        IRNode::PrefixUnaryExpr { operand, .. } => {
            rewrite_exported_var_refs(operand, ns_name, names);
        }
        IRNode::PostfixUnaryExpr { operand, .. } => {
            rewrite_exported_var_refs(operand, ns_name, names);
        }
        IRNode::CallExpr {
            callee, arguments, ..
        } => {
            rewrite_exported_var_refs(callee, ns_name, names);
            for arg in arguments {
                rewrite_exported_var_refs(arg, ns_name, names);
            }
        }
        IRNode::NewExpr {
            callee, arguments, ..
        } => {
            rewrite_exported_var_refs(callee, ns_name, names);
            for arg in arguments {
                rewrite_exported_var_refs(arg, ns_name, names);
            }
        }
        IRNode::PropertyAccess { object, .. } => rewrite_exported_var_refs(object, ns_name, names),
        IRNode::ElementAccess { object, index } => {
            rewrite_exported_var_refs(object, ns_name, names);
            rewrite_exported_var_refs(index, ns_name, names);
        }
        IRNode::ConditionalExpr {
            condition,
            when_true,
            when_false,
            ..
        } => {
            rewrite_exported_var_refs(condition, ns_name, names);
            rewrite_exported_var_refs(when_true, ns_name, names);
            rewrite_exported_var_refs(when_false, ns_name, names);
        }
        IRNode::Parenthesized(inner) => rewrite_exported_var_refs(inner, ns_name, names),
        IRNode::CommaExpr(exprs) | IRNode::ArrayLiteral(exprs) => {
            for expr in exprs.iter_mut() {
                rewrite_exported_var_refs(expr, ns_name, names);
            }
        }
        IRNode::SpreadElement(inner) => rewrite_exported_var_refs(inner, ns_name, names),
        IRNode::ObjectLiteral { properties, .. } => {
            for prop in properties.iter_mut() {
                if let IRPropertyKey::Computed(key) = &mut prop.key {
                    rewrite_exported_var_refs(key, ns_name, names);
                }
                rewrite_exported_var_refs(&mut prop.value, ns_name, names);
            }
        }
        IRNode::FunctionExpr { body, .. } | IRNode::FunctionDecl { body, .. } => {
            for stmt in body {
                rewrite_exported_var_refs(stmt, ns_name, names);
            }
        }
        IRNode::LogicalOr { left, right } | IRNode::LogicalAnd { left, right } => {
            rewrite_exported_var_refs(left, ns_name, names);
            rewrite_exported_var_refs(right, ns_name, names);
        }
        IRNode::VarDecl {
            initializer: Some(initializer),
            ..
        } => {
            rewrite_exported_var_refs(initializer, ns_name, names);
        }
        IRNode::VarDeclList(items) => {
            for item in items {
                rewrite_exported_var_refs(item, ns_name, names);
            }
        }
        IRNode::ExpressionStatement(expr) => {
            rewrite_exported_var_refs(expr, ns_name, names);
        }
        IRNode::ReturnStatement(Some(expr)) | IRNode::ThrowStatement(expr) => {
            rewrite_exported_var_refs(expr, ns_name, names);
        }
        IRNode::AwaiterCall {
            this_arg,
            generator_body,
            ..
        } => {
            rewrite_exported_var_refs(this_arg, ns_name, names);
            rewrite_exported_var_refs(generator_body, ns_name, names);
        }
        IRNode::IfStatement {
            condition,
            then_branch,
            else_branch,
        } => {
            rewrite_exported_var_refs(condition, ns_name, names);
            rewrite_exported_var_refs(then_branch, ns_name, names);
            if let Some(else_branch) = else_branch {
                rewrite_exported_var_refs(else_branch, ns_name, names);
            }
        }
        IRNode::Block(statements) | IRNode::Sequence(statements) => {
            for stmt in statements {
                rewrite_exported_var_refs(stmt, ns_name, names);
            }
        }
        IRNode::ForStatement {
            initializer,
            condition,
            incrementor,
            body,
        } => {
            if let Some(init) = initializer {
                rewrite_exported_var_refs(init, ns_name, names);
            }
            if let Some(cond) = condition {
                rewrite_exported_var_refs(cond, ns_name, names);
            }
            if let Some(inc) = incrementor {
                rewrite_exported_var_refs(inc, ns_name, names);
            }
            rewrite_exported_var_refs(body, ns_name, names);
        }
        IRNode::WhileStatement { condition, body }
        | IRNode::DoWhileStatement {
            body: condition,
            condition: body,
            ..
        } => {
            rewrite_exported_var_refs(condition, ns_name, names);
            rewrite_exported_var_refs(body, ns_name, names);
        }
        IRNode::TryStatement {
            try_block,
            catch_clause,
            finally_block,
        } => {
            rewrite_exported_var_refs(try_block, ns_name, names);
            if let Some(catch) = catch_clause {
                if let Some(param) = catch.param.as_ref() {
                    let _ = param;
                }
                for stmt in &mut catch.body {
                    rewrite_exported_var_refs(stmt, ns_name, names);
                }
            }
            if let Some(finally_block) = finally_block {
                rewrite_exported_var_refs(finally_block, ns_name, names);
            }
        }
        IRNode::LabeledStatement { statement, .. } => {
            rewrite_exported_var_refs(statement, ns_name, names);
        }
        IRNode::SwitchStatement { expression, cases } => {
            rewrite_exported_var_refs(expression, ns_name, names);
            for case in cases {
                if let Some(ref mut test) = case.test {
                    rewrite_exported_var_refs(test, ns_name, names);
                }
                for stmt in &mut case.statements {
                    rewrite_exported_var_refs(stmt, ns_name, names);
                }
            }
        }
        IRNode::NamespaceIIFE { body, .. } => {
            for stmt in body {
                rewrite_exported_var_refs(stmt, ns_name, names);
            }
        }
        IRNode::NamespaceExport { value, .. } => {
            rewrite_exported_var_refs(value, ns_name, names);
        }
        IRNode::ES5ClassIIFE { body, .. } => {
            for stmt in body {
                rewrite_exported_var_refs(stmt, ns_name, names);
            }
        }
        IRNode::PrototypeMethod { function, .. } | IRNode::StaticMethod { function, .. } => {
            rewrite_exported_var_refs(function, ns_name, names);
        }
        IRNode::DefineProperty {
            target, descriptor, ..
        } => {
            rewrite_exported_var_refs(target, ns_name, names);
            if let Some(getter) = &mut descriptor.get {
                rewrite_exported_var_refs(getter, ns_name, names);
            }
            if let Some(setter) = &mut descriptor.set {
                rewrite_exported_var_refs(setter, ns_name, names);
            }
        }
        IRNode::EnumIIFE { members, .. } => {
            for member in members {
                if let EnumMemberValue::Computed(expr) = &mut member.value {
                    rewrite_exported_var_refs(expr, ns_name, names);
                }
            }
        }
        _ => {}
    }
}

fn collect_qualified_name_parts(arena: &NodeArena, name_idx: NodeIndex) -> Option<Vec<String>> {
    let node = arena.get(name_idx)?;

    if node.kind == SyntaxKind::Identifier as u16 {
        if let Some(id) = arena.get_identifier(node) {
            return Some(vec![id.escaped_text.clone()]);
        }
        return None;
    }

    if node.kind == syntax_kind_ext::QUALIFIED_NAME
        && let Some(qn) = arena.qualified_names.get(node.data_index as usize)
    {
        let mut left = collect_qualified_name_parts(arena, qn.left)?;
        let right = collect_qualified_name_parts(arena, qn.right)?;
        left.extend(right);
        return Some(left);
    }

    None
}

fn namespace_body_by_name(arena: &NodeArena, target_parts: &[String]) -> Option<NodeIndex> {
    if target_parts.is_empty() {
        return None;
    }

    for (idx, node) in arena.nodes.iter().enumerate() {
        if node.kind != syntax_kind_ext::MODULE_DECLARATION {
            continue;
        };

        if let Some((parts, body_idx)) =
            collect_module_decl_parts_for_body_lookup(arena, NodeIndex(idx as u32))
            && parts == target_parts
        {
            return Some(body_idx);
        }
    }

    None
}

fn collect_module_decl_parts_for_body_lookup(
    arena: &NodeArena,
    ns_idx: NodeIndex,
) -> Option<(Vec<String>, NodeIndex)> {
    let mut parts = Vec::new();
    let mut current_idx = ns_idx;

    loop {
        let node = arena.get(current_idx)?;
        if node.kind != syntax_kind_ext::MODULE_DECLARATION {
            break;
        }

        let ns_data = arena.get_module(node)?;

        if let Some(name_node) = arena.get(ns_data.name)
            && let Some(id) = arena.get_identifier(name_node)
        {
            parts.push(id.escaped_text.clone());
        }

        let body_node = arena.get(ns_data.body)?;
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            current_idx = ns_data.body;
        } else {
            return Some((parts, ns_data.body));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some((parts, current_idx))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../../tests/namespace_es5_ir.rs"]
mod tests;
