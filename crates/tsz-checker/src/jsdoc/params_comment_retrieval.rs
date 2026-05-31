//! JSDoc comment retrieval and type cast helpers.
//!
//! This module owns:
//! - JSDoc comment position/content lookup (ancestor walk, leading comment search)
//! - Type-cast comment helpers
//! - Override tag detection
//! - `@satisfies` JSDoc comment detection

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the byte position of the JSDoc comment for a function node.
    ///
    /// Returns `Some(pos)` where pos is the byte offset of `/**` in the source.
    pub(crate) fn get_jsdoc_comment_pos_for_function(&self, func_idx: NodeIndex) -> Option<u32> {
        use tsz_common::comments::is_jsdoc_comment;

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let func_node = self.ctx.arena.get(func_idx)?;

        // Check inline JSDoc
        if let Some(comment) = comments
            .iter()
            .find(|c| c.pos <= func_node.pos && func_node.pos < c.end)
            && is_jsdoc_comment(comment, source_text)
        {
            return Some(comment.pos);
        }

        // Check leading comments
        for comment in comments.iter().rev() {
            if comment.end <= func_node.pos && is_jsdoc_comment(comment, source_text) {
                // Check that there's nothing but whitespace between comment and node
                let between = &source_text[comment.end as usize..func_node.pos as usize];
                if between.trim().is_empty() {
                    return Some(comment.pos);
                }
            }
        }

        if func_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION {
            // For `export function f(...)`, the JSDoc is before `export` but
            // func_node.pos is at `function`. Check the parent ExportDeclaration.
            if let Some(ext) = self.ctx.arena.get_extended(func_idx)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION
            {
                for comment in comments.iter().rev() {
                    if comment.end <= parent_node.pos && is_jsdoc_comment(comment, source_text) {
                        let between = &source_text[comment.end as usize..parent_node.pos as usize];
                        if between.trim().is_empty() {
                            return Some(comment.pos);
                        }
                    }
                }
            }
            return None;
        }

        // Walk up parent chain (for const f = ...)
        let mut current = func_idx;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            for comment in comments.iter().rev() {
                if comment.end <= parent_node.pos && is_jsdoc_comment(comment, source_text) {
                    let between = &source_text[comment.end as usize..parent_node.pos as usize];
                    if between.trim().is_empty() {
                        return Some(comment.pos);
                    }
                }
            }
            current = parent;
        }

        None
    }

    /// Get the `JSDoc` comment content for a function node.
    ///
    /// Walks up the parent chain from the function node to find the `JSDoc`
    /// comment. For variable-assigned functions (e.g., `const f = () => {}`),
    /// the `JSDoc` is on the variable statement, not the function itself.
    ///
    /// Returns the raw `JSDoc` content (without `/**` and `*/` delimiters).
    pub(crate) fn get_jsdoc_for_function(&self, func_idx: NodeIndex) -> Option<String> {
        if self.is_js_file() && !self.ctx.compiler_options.check_js {
            return None;
        }
        self.find_jsdoc_for_function(func_idx)
    }

    /// Resolve a function's JSDoc `@type` annotation into a callable type when
    /// it actually carries call signatures (including callback typedef aliases).
    ///
    /// Broad object-ish annotations like `Function` should not count here
    /// because they do not provide concrete parameter types and should still
    /// allow TS7006 to fire.
    pub(crate) fn jsdoc_callable_type_annotation_for_function(
        &mut self,
        func_idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }

        let sf = self.source_file_data_for_node(func_idx)?;
        if sf.comments.is_empty() || !sf.comments.iter().any(|comment| comment.is_multi_line) {
            return None;
        }

        let node = self.ctx.arena.get(func_idx)?;
        let jsdoc = self.get_jsdoc_for_function(func_idx)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        self.jsdoc_concrete_callable_type_from_expr(type_expr, node.pos, &comments, &source_text)
    }

    pub(crate) fn jsdoc_type_tag_references_callback_typedef(
        &self,
        func_idx: NodeIndex,
        jsdoc: &str,
    ) -> bool {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(type_expr) = Self::jsdoc_extract_type_tag_expr(jsdoc) else {
            return false;
        };
        let Some(sf) = self.source_file_data_for_node(func_idx) else {
            return false;
        };

        // Scan all comments — @callback/@typedef are hoisted to file scope
        // in tsc, so forward references must be supported.
        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, &sf.text) {
                continue;
            }
            let content = get_jsdoc_content(comment, &sf.text);
            if Self::parse_jsdoc_typedefs(&content)
                .into_iter()
                .any(|(name, info)| name == type_expr && info.callback.is_some())
            {
                return true;
            }
        }

        false
    }

    /// Find the JSDoc comment for a function node without checking compiler options.
    ///
    /// Used by `get_jsdoc_for_function` (which adds a `check_js` guard) and by
    /// TS8024 validation which needs JSDoc lookup independent of the checker's
    /// `check_js` state (the driver-level `check_js` controls JS file inclusion).
    pub(crate) fn find_jsdoc_for_function(&self, func_idx: NodeIndex) -> Option<String> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        if !comments.iter().any(|comment| comment.is_multi_line) {
            return None;
        }

        // Try the function node itself first
        let func_node = self.ctx.arena.get(func_idx)?;

        // For inline JSDoc (comment overlapping with node position)
        if let Some(comment) = comments
            .iter()
            .find(|c| c.pos <= func_node.pos && func_node.pos < c.end)
            && is_jsdoc_comment(comment, source_text)
        {
            return Some(get_jsdoc_content(comment, source_text));
        }

        if func_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(jsdoc) = self.try_leading_jsdoc(comments, func_node.pos, source_text) {
                return Some(jsdoc);
            }
            // For `export function f(...)`, the JSDoc is before the `export` keyword
            // but func_node.pos is at `function`. Walk up to the parent
            // (ExportDeclaration) to find the JSDoc there.
            if let Some(ext) = self.ctx.arena.get_extended(func_idx)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION
            {
                return self.try_leading_jsdoc(comments, parent_node.pos, source_text);
            }
            return None;
        }

        // When this function is the expression body of another function/arrow,
        // the parent's JSDoc belongs to the parent — not to this nested function.
        // E.g. `/** @template T @returns {(b: T) => T} */ const seq = a => b => b;`
        // The JSDoc belongs to the outer arrow `a => ...`, not to the inner `b => b`.
        // Without this guard, the ancestor walk would reach the variable declaration
        // and incorrectly assign @template/@returns to the inner arrow.
        if let Some(ext) = self.ctx.arena.get_extended(func_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && matches!(
                parent_node.kind,
                tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                    | tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                    | tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                    | tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION
            )
        {
            return None;
        }

        // Try leading comments, then walk up the parent chain for
        // `const f = value => ...` where JSDoc is on the `const` line.
        self.try_jsdoc_with_ancestor_walk(func_idx, comments, source_text)
    }

    /// Try to find a leading JSDoc comment for a node, walking up to 4 ancestors.
    ///
    /// First checks `idx` itself, then walks the parent chain up to 4 levels.
    /// Returns the first JSDoc content found, or `None`.
    pub(crate) fn effective_jsdoc_pos_for_node(
        &self,
        idx: NodeIndex,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<u32> {
        let node = self.ctx.arena.get(idx)?;
        let mut pos = node.pos as usize;
        let end = node.end as usize;

        while pos < end {
            let remaining = source_text.get(pos..end)?;
            let trimmed = remaining.trim_start_matches(char::is_whitespace);
            if trimmed.len() != remaining.len() {
                pos += remaining.len() - trimmed.len();
                continue;
            }

            if let Some(comment) = comments
                .iter()
                .find(|comment| comment.pos as usize == pos && comment.end as usize <= end)
            {
                pos = comment.end as usize;
                continue;
            }

            break;
        }

        Some(pos as u32)
    }

    pub(crate) fn try_jsdoc_with_ancestor_walk(
        &self,
        idx: NodeIndex,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<String> {
        let jsdoc = self.try_leading_jsdoc(
            comments,
            self.effective_jsdoc_pos_for_node(idx, comments, source_text)?,
            source_text,
        );
        if jsdoc.is_some() {
            return jsdoc;
        }
        let mut current = idx;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            // Stop before checking statement-level containers whose "leading
            // JSDoc" belongs to their first child statement, not to the node
            // we started the walk from. Without this guard, `var res` in:
            //   /** @type {Foo} */ export const x = ...
            //   var res = x()
            // would inherit Foo through SourceFile's leading-comment position.
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                use tsz_parser::parser::syntax_kind_ext as sk;
                if matches!(
                    parent_node.kind,
                    sk::SOURCE_FILE
                        | sk::BLOCK
                        | sk::MODULE_BLOCK
                        | sk::CASE_CLAUSE
                        | sk::DEFAULT_CLAUSE
                ) {
                    break;
                }
            }
            let jsdoc = self.try_leading_jsdoc(
                comments,
                self.effective_jsdoc_pos_for_node(parent, comments, source_text)?,
                source_text,
            );
            if jsdoc.is_some() {
                return jsdoc;
            }
            current = parent;
        }
        None
    }

    /// Like `try_jsdoc_with_ancestor_walk` but also returns the absolute start
    /// position of the JSDoc comment in the source file.
    ///
    /// This is needed for `@satisfies` to compute the `@satisfies` keyword offset.
    pub(crate) fn try_jsdoc_with_ancestor_walk_and_pos(
        &self,
        idx: NodeIndex,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<(String, u32)> {
        if let Some((content, pos)) = self.try_leading_jsdoc_with_pos(
            comments,
            self.effective_jsdoc_pos_for_node(idx, comments, source_text)?,
            source_text,
        ) {
            return Some((content, pos));
        }
        let mut current = idx;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            // Same container guard as try_jsdoc_with_ancestor_walk.
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                use tsz_parser::parser::syntax_kind_ext as sk;
                if matches!(
                    parent_node.kind,
                    sk::SOURCE_FILE
                        | sk::BLOCK
                        | sk::MODULE_BLOCK
                        | sk::CASE_CLAUSE
                        | sk::DEFAULT_CLAUSE
                ) {
                    break;
                }
            }
            if let Some((content, pos)) = self.try_leading_jsdoc_with_pos(
                comments,
                self.effective_jsdoc_pos_for_node(parent, comments, source_text)?,
                source_text,
            ) {
                return Some((content, pos));
            }
            current = parent;
        }
        None
    }

    /// Try to find a leading `JSDoc` comment and its start position.
    pub(crate) fn try_leading_jsdoc_with_pos(
        &self,
        comments: &[tsz_common::comments::CommentRange],
        pos: u32,
        source_text: &str,
    ) -> Option<(String, u32)> {
        use tsz_common::comments::{
            get_jsdoc_content, get_leading_comments_from_cache, is_jsdoc_comment,
        };

        let leading = get_leading_comments_from_cache(comments, pos, source_text);
        for comment in leading.iter().rev() {
            let end = comment.end as usize;
            let check = pos as usize;
            if end <= check
                && source_text
                    .get(end..check)
                    .is_some_and(|gap| gap.chars().all(char::is_whitespace))
                && is_jsdoc_comment(comment, source_text)
            {
                return Some((get_jsdoc_content(comment, source_text), comment.pos));
            }
        }
        None
    }

    /// Check if `idx` or an ancestor has a leading JSDoc comment containing `@satisfies`.
    ///
    /// This is used by contextual typing code paths that need to treat inline JSDoc
    /// wrappers like `/** @satisfies ... */ expr` as an explicit typing boundary.
    pub(crate) fn has_satisfies_jsdoc_comment(&self, idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        if let Some(jsdoc) = self.try_jsdoc_with_ancestor_walk(idx, comments, source_text) {
            return Self::jsdoc_contains_tag(&jsdoc, "satisfies");
        }

        false
    }

    /// Check if a parenthesized expression node has an immediate leading JSDoc
    /// `@type {T}` cast. Direct lookup only — does not walk ancestors, since a
    /// JSDoc cast must be attached right before the `(` it casts. Used by
    /// `literal_type_from_initializer` to short-circuit walking through a
    /// JSDoc-cast paren when extracting fresh literal types.
    pub(crate) fn paren_has_jsdoc_type_cast(&self, idx: NodeIndex) -> bool {
        if !self.ctx.should_resolve_jsdoc() {
            return false;
        }
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        if sf.comments.is_empty() || !sf.comments.iter().any(|c| c.is_multi_line) {
            return false;
        }
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(pos) = self.effective_jsdoc_pos_for_node(idx, comments, source_text) else {
            return false;
        };
        let Some(jsdoc) = self.try_leading_jsdoc(comments, pos, source_text) else {
            return false;
        };
        Self::jsdoc_contains_tag(&jsdoc, "type")
    }

    /// Extract the type expression text from a leading `@satisfies {TypeExpr}` JSDoc comment
    /// on `idx` or an ancestor. Returns the raw type text (e.g. `Record<Keys, unknown>`).
    pub(crate) fn jsdoc_satisfies_type_text_for_node(&self, idx: NodeIndex) -> Option<String> {
        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let jsdoc = self.try_jsdoc_with_ancestor_walk(idx, comments, source_text)?;
        let type_expr = Self::extract_jsdoc_satisfies_expression(&jsdoc)?;
        Some(type_expr.to_owned())
    }

    /// Try to find a leading `JSDoc` comment before a given position.
    pub(crate) fn try_leading_jsdoc(
        &self,
        comments: &[tsz_common::comments::CommentRange],
        pos: u32,
        source_text: &str,
    ) -> Option<String> {
        use tsz_common::comments::{
            get_jsdoc_content, get_leading_comments_from_cache, is_jsdoc_comment,
        };

        let leading = get_leading_comments_from_cache(comments, pos, source_text);
        for comment in leading.iter().rev() {
            let end = comment.end as usize;
            let check = pos as usize;
            if end <= check
                && source_text
                    .get(end..check)
                    .is_some_and(|gap| gap.chars().all(char::is_whitespace))
                && is_jsdoc_comment(comment, source_text)
            {
                return Some(get_jsdoc_content(comment, source_text));
            }
        }
        None
    }

    /// Check if a parameter node has an inline `/** @type {T} */` `JSDoc` annotation.
    ///
    /// In TypeScript, parameters can have inline `JSDoc` type annotations like:
    ///   `function foo(/** @type {string} */ msg, /** @type {number} */ count)`
    /// These annotations suppress TS7006 because the parameter type is provided via `JSDoc`.
    pub(crate) fn param_has_inline_jsdoc_type(&self, param_idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let param_node = match self.ctx.arena.get(param_idx) {
            Some(n) => n,
            None => return false,
        };

        // Look for a JSDoc comment that ends right before or overlaps the parameter position
        if let Some(content) = self.try_leading_jsdoc(comments, param_node.pos, source_text) {
            return Self::jsdoc_contains_tag(&content, "type");
        }

        false
    }

    /// Check if a node is inside a JSDoc `@type` cast parenthesized expression.
    ///
    /// Walks up the parent chain looking for a `PARENTHESIZED_EXPRESSION` with a
    /// leading `/** @type {...} */` JSDoc comment. This is used to suppress TS7006
    /// for arrow/function parameters inside JSDoc type casts like:
    ///   `/** @type {import("./foo").Bar} */({ doer: q => q })`
    ///
    /// Even if the import type can't be fully resolved, the explicit @type
    /// annotation means the user intended to type the expression.
    pub(crate) fn is_inside_jsdoc_type_cast(&self, idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let mut current = idx;
        // Walk up at most 8 levels to find an enclosing JSDoc @type cast.
        // Typical nesting: arrow -> (params) -> property -> obj_literal -> paren_expr
        for _ in 0..8 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(jsdoc) = self.try_leading_jsdoc(comments, parent_node.pos, source_text)
                && Self::jsdoc_contains_tag(&jsdoc, "type")
            {
                // Issue #3956: `/** @type {*} */(expr)` (and the related
                // broad casts `any`, `unknown`, `Object`, `Function`) do
                // NOT provide a contextual parameter type for nested
                // closures. tsc still reports TS7006 for closure params
                // in those cases. Only suppress when the cast type
                // could plausibly contribute a contextual signature.
                if let Some(type_expr) = Self::extract_jsdoc_type_expression(&jsdoc)
                    && Self::jsdoc_type_cast_is_broad(type_expr.trim())
                {
                    return false;
                }
                return true;
            }
            current = parent;
        }
        false
    }

    /// Whether a JSDoc `@type` cast type expression is "broad" — i.e.
    /// it does not constrain nested closure parameters and so cannot
    /// suppress TS7006 implicit-any diagnostics on them.
    ///
    /// Mirrors tsc's handling of `*`, `any`, `unknown`, `Object`,
    /// and `Function` as cast types whose contextual contribution to
    /// nested closure parameters is empty.
    fn jsdoc_type_cast_is_broad(type_expr: &str) -> bool {
        matches!(
            type_expr,
            "*" | "?" | "any" | "unknown" | "Object" | "object" | "Function"
        )
    }

    /// Check if a node has a `/** @override */` JSDoc annotation.
    pub(crate) fn has_jsdoc_override_tag(&self, idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        self.try_jsdoc_with_ancestor_walk(idx, comments, source_text)
            .is_some_and(|content| Self::jsdoc_contains_tag(&content, "override"))
    }
}
