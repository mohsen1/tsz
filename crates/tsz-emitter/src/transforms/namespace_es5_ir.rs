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

#[path = "namespace_es5_ir_helpers.rs"]
mod namespace_es5_ir_helpers;
use namespace_es5_ir_helpers::*;

use crate::transforms::class_es5_ir::{AstToIr, ES5ClassTransformer};
use crate::transforms::enum_es5_ir::transform_enum_to_ir;
use crate::transforms::ir::{EnumMemberValue, IRNode, IRParam, IRPropertyKey};
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
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            is_commonjs: false,
            source_text: None,
            comment_ranges: Vec::new(),
        }
    }

    /// Create a namespace transformer with `CommonJS` mode enabled
    pub const fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
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

    /// Set `CommonJS` mode
    pub const fn set_commonjs(&mut self, is_commonjs: bool) {
        self.is_commonjs = is_commonjs;
    }

    /// Extract leading comments from source text that fall within [`from_pos`, `to_pos`) range.
    /// Returns `IRNode::Raw` nodes since the text already includes comment delimiters.
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

    /// Extract standalone comments (on their own line) within [`from_pos`, `to_pos`).
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
    /// * `ns_idx` - `NodeIndex` of the namespace declaration
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
    /// Use this when the namespace is wrapped in an `EXPORT_DECLARATION`.
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
    /// For qualified names like `A.B.C` (parsed as nested `MODULE_DECLARATIONs`), returns `["A", "B", "C"]`.
    /// For simple identifiers like `foo`, returns `["foo"]`.
    ///
    /// Note: The parser creates nested `MODULE_DECLARATION` nodes for qualified namespace names,
    /// where each level has a single identifier name and the body points to the next level.
    pub fn flatten_module_name(&self, name_idx: NodeIndex) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        self.collect_name_parts(name_idx, &mut parts);
        if parts.is_empty() { None } else { Some(parts) }
    }

    /// Recursively collect name parts from qualified names
    ///
    /// Handles both:
    /// 1. `QUALIFIED_NAME` nodes (left.right structure)
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

    /// Collect all name parts by walking through nested `MODULE_DECLARATION` chain
    ///
    /// For `namespace A.B.C {}`, the parser creates:
    /// `MODULE_DECLARATION` "A" -> body: `MODULE_DECLARATION` "B" -> body: `MODULE_DECLARATION` "C" -> body: `MODULE_BLOCK`
    ///
    /// This method walks through all levels and returns (["A", "B", "C"], `innermost_body_idx`)
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
                    value: Box::new(IRNode::Identifier(func_name)),
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

        let is_exported = has_export_modifier(self.arena, &enum_data.modifiers);

        let mut enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;

        // For exported enums, fold the namespace export into the IIFE closing:
        // `(Color = A.Color || (A.Color = {}))` instead of separate `A.Color = Color;`
        if is_exported
            && let IRNode::EnumIIFE {
                namespace_export, ..
            } = &mut enum_ir
        {
            *namespace_export = Some(ns_name.to_string());
        }

        Some(enum_ir)
    }

    /// Transform an exported enum
    fn transform_enum_exported(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let mut enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;

        // Fold namespace export into IIFE closing
        if let IRNode::EnumIIFE {
            namespace_export, ..
        } = &mut enum_ir
        {
            *namespace_export = Some(ns_name.to_string());
        }

        Some(enum_ir)
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

/// Context for namespace transformation (legacy, use `NamespaceES5Transformer` instead)
pub struct NamespaceTransformContext<'a> {
    arena: &'a NodeArena,
    is_commonjs: bool,
}

impl<'a> NamespaceTransformContext<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            is_commonjs: false,
        }
    }

    pub const fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
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

    const fn namespace_member_ast_ref_if_non_empty(&self, member_idx: NodeIndex) -> Option<IRNode> {
        Some(IRNode::ASTRef(member_idx))
    }

    /// Collect all name parts by walking through nested `MODULE_DECLARATION` chain
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
                    value: Box::new(IRNode::Identifier(func_name)),
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

        let is_exported = has_export_modifier(self.arena, &enum_data.modifiers);

        let mut enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;

        // For exported enums, fold the namespace export into the IIFE closing
        if is_exported
            && let IRNode::EnumIIFE {
                namespace_export, ..
            } = &mut enum_ir
        {
            *namespace_export = Some(ns_name.to_string());
        }

        Some(enum_ir)
    }

    /// Transform an exported enum in namespace
    fn transform_enum_in_namespace_exported(
        &self,
        ns_name: &str,
        enum_idx: NodeIndex,
    ) -> Option<IRNode> {
        let mut enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;

        // Fold namespace export into IIFE closing
        if let IRNode::EnumIIFE {
            namespace_export, ..
        } = &mut enum_ir
        {
            *namespace_export = Some(ns_name.to_string());
        }

        Some(enum_ir)
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
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../../tests/namespace_es5_ir.rs"]
mod tests;
