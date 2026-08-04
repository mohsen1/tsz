//! The TS1356 `Did you mean to mark this function as 'async'?` pointer that
//! `tsc` attaches to an `await`-outside-`async` grammar error.
//!
//! `tsc`'s `checkAwaitExpression` (TS1308) and `checkGrammarForInOrOfStatement`
//! (TS1103) both build their diagnostic, then look up
//! `getContainingFunction(node)` and — when that container exists, is not a
//! constructor, and does not carry `async` — attach
//! `Did_you_mean_to_mark_this_function_as_async` as `relatedInformation`
//! anchored on `getErrorSpanForNode(container)`.
//!
//! It is a cross-location pointer, not a message-chain link: `tsc --pretty`
//! prints it with its own location and snippet while `tsc --pretty false`
//! prints nothing for it. So it is tagged
//! [`RelatedInformationKind::LocationPointer`](tsz_common::diagnostics::RelatedInformationKind::LocationPointer)
//! and plain-mode output is unchanged by it.

use crate::diagnostics::{Diagnostic, DiagnosticRelatedInformation, diagnostic_codes};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// How far up the parent chain the containing-function search walks before
/// giving up. `getContainingFunction` is unbounded in `tsc`; the walk here is
/// capped so a malformed parent chain cannot spin, and the cap is far above
/// the nesting any real expression puts between an `await` and its function
/// (parenthesis/binary/call/argument levels).
const CONTAINING_FUNCTION_WALK_LIMIT: usize = 256;

impl CheckerState<'_> {
    /// The TS1356 related-information entry for an `await` grammar error at
    /// `node_idx`, or an empty vector when `tsc` attaches none.
    ///
    /// Empty for a top-level `await` (no containing function), for a
    /// constructor body (`tsc` excludes `SyntaxKind.Constructor` explicitly),
    /// and for an `async` container — the last one is reachable through a
    /// position that does not inherit the container's await context, such as a
    /// parameter initializer.
    pub(crate) fn did_you_mean_async_related(
        &self,
        node_idx: NodeIndex,
    ) -> Vec<DiagnosticRelatedInformation> {
        let Some(func_idx) = self.containing_function_like(node_idx) else {
            return Vec::new();
        };
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return Vec::new();
        };
        if func_node.kind == syntax_kind_ext::CONSTRUCTOR || self.function_like_is_async(func_idx) {
            return Vec::new();
        }
        let Some((start, length)) = self.function_like_error_span(func_idx) else {
            return Vec::new();
        };
        vec![Diagnostic::related_pointer(
            diagnostic_codes::DID_YOU_MEAN_TO_MARK_THIS_FUNCTION_AS_ASYNC,
            self.ctx.file_name.clone(),
            start,
            length,
            crate::diagnostics::diagnostic_messages::DID_YOU_MEAN_TO_MARK_THIS_FUNCTION_AS_ASYNC,
        )]
    }

    /// `tsc`'s `getContainingFunction`: the nearest function-like ancestor,
    /// arrows and accessors included, constructors included (the caller
    /// filters those out the way `tsc` does).
    fn containing_function_like(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..CONTAINING_FUNCTION_WALK_LIMIT {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if matches!(
                parent_node.kind,
                syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
            ) {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    /// Whether a function-like node carries `async`.
    fn function_like_is_async(&self, func_idx: NodeIndex) -> bool {
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        if let Some(func) = self.ctx.arena.get_function(func_node) {
            return func.is_async;
        }
        if let Some(method) = self.ctx.arena.get_method_decl(func_node) {
            return self
                .ctx
                .arena
                .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword);
        }
        self.ctx
            .arena
            .get_accessor(func_node)
            .is_some_and(|accessor| {
                self.ctx
                    .arena
                    .has_modifier(&accessor.modifiers, SyntaxKind::AsyncKeyword)
            })
    }

    /// `tsc`'s `getErrorSpanForNode` for a function-like node.
    ///
    /// A named declaration answers with its name (a computed member name
    /// included, as the whole `[expr]` node). An arrow answers through
    /// [`Self::arrow_error_span`]. An anonymous function expression answers
    /// with the name it is assigned to, when it has one, and otherwise with
    /// its first token — the `function` keyword.
    fn function_like_error_span(&self, func_idx: NodeIndex) -> Option<(u32, u32)> {
        let func_node = self.ctx.arena.get(func_idx)?;
        if func_node.kind == syntax_kind_ext::ARROW_FUNCTION {
            return self.arrow_error_span(func_idx);
        }

        let name_idx = self.function_like_name(func_idx);
        if name_idx.is_some()
            && let Some(span) = self.name_node_span(name_idx)
        {
            return Some(span);
        }

        if let Some(assigned) = self.assigned_name_span(func_idx) {
            return Some(assigned);
        }

        let (start, end) = self.get_node_span(func_idx)?;
        let keyword_len = tsz_scanner::keyword_text_len(SyntaxKind::FunctionKeyword);
        Some((start, keyword_len.min(end.saturating_sub(start))))
    }

    /// The declared name node of a function-like, for the kinds that have one.
    fn function_like_name(&self, func_idx: NodeIndex) -> NodeIndex {
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return NodeIndex::NONE;
        };
        if let Some(func) = self.ctx.arena.get_function(func_node) {
            return func.name;
        }
        if let Some(method) = self.ctx.arena.get_method_decl(func_node) {
            return method.name;
        }
        self.ctx
            .arena
            .get_accessor(func_node)
            .map_or(NodeIndex::NONE, |accessor| accessor.name)
    }

    /// `tsc`'s `getAssignedName`: the name an anonymous function expression is
    /// bound to by its immediate parent.
    ///
    /// Covers the four parent shapes `tsc` covers — a variable declaration, a
    /// binding element, an object-literal property assignment, and the right
    /// operand of an assignment. A class property initializer is deliberately
    /// **not** one of them (`class C { p = function () {} }` answers with the
    /// `function` keyword, oracle-confirmed), so this reads the parent kind
    /// rather than looking for any enclosing named declaration.
    fn assigned_name_span(&self, func_idx: NodeIndex) -> Option<(u32, u32)> {
        let parent_idx = self.ctx.arena.get_extended(func_idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;

        let name_idx = match parent_node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.ctx.arena.get_variable_declaration(parent_node)?;
                (decl.initializer == func_idx).then_some(decl.name)?
            }
            syntax_kind_ext::BINDING_ELEMENT => {
                let element = self.ctx.arena.get_binding_element(parent_node)?;
                (element.initializer == func_idx).then_some(element.name)?
            }
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let assignment = self.ctx.arena.get_property_assignment(parent_node)?;
                (assignment.initializer == func_idx).then_some(assignment.name)?
            }
            syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.ctx.arena.get_binary_expr(parent_node)?;
                if binary.right != func_idx
                    || binary.operator_token != SyntaxKind::EqualsToken as u16
                    || !self.is_assignment_target_name(binary.left)
                {
                    return None;
                }
                binary.left
            }
            _ => return None,
        };

        self.name_node_span(name_idx)
    }

    /// The span of a declaration name node, through the shared anchor policy
    /// so an identifier is trimmed to the identifier itself rather than to the
    /// token that follows it, and a computed name keeps its whole `[expr]`.
    fn name_node_span(&self, name_idx: NodeIndex) -> Option<(u32, u32)> {
        let (start, end) = self.get_node_span(name_idx)?;
        Some(self.normalized_anchor_span(name_idx, start, end.saturating_sub(start)))
    }

    /// Whether an assignment's left operand is a name `tsc`'s `getAssignedName`
    /// would answer with: a bare identifier or an access expression.
    fn is_assignment_target_name(&self, left_idx: NodeIndex) -> bool {
        self.ctx.arena.get(left_idx).is_some_and(|left| {
            left.kind == SyntaxKind::Identifier as u16
                || left.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || left.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        })
    }

    /// `tsc`'s `getErrorSpanForArrowFunction`: the whole arrow, except that an
    /// arrow whose block body spans more than one line is trimmed to its
    /// header — everything up to and including the body's opening brace — so
    /// the pointer stays on one line.
    fn arrow_error_span(&self, arrow_idx: NodeIndex) -> Option<(u32, u32)> {
        let (start, end) = self.get_node_span(arrow_idx)?;
        let arrow_node = self.ctx.arena.get(arrow_idx)?;
        let body_idx = self
            .ctx
            .arena
            .get_function(arrow_node)
            .map_or(NodeIndex::NONE, |func| func.body);
        let body_span = self
            .ctx
            .arena
            .get(body_idx)
            .and_then(|body_node| Some((body_node.kind, self.get_node_span(body_idx)?)));

        if let Some((body_kind, (body_start, body_end))) = body_span
            && body_kind == syntax_kind_ext::BLOCK
            && self.span_spans_multiple_lines(body_start, body_end)
            && body_start >= start
        {
            return Some((start, body_start.saturating_sub(start) + 1));
        }

        Some((
            start,
            self.trimmed_span_end(start, end).saturating_sub(start),
        ))
    }

    /// An arrow's stored `end` runs to the end of the token that closes the
    /// construct it sits in, so a statement's `;` lands inside the node's span
    /// (the same over-extension family as the close-brace positions fixed in
    /// tsz#16259, recorded for this position in tsz#16360). `tsc` anchors on the arrow's own last token, so the span is
    /// trimmed back over trailing separators and whitespace before it is used
    /// as a pointer.
    fn trimmed_span_end(&self, start: u32, end: u32) -> u32 {
        let Some(text) = self
            .ctx
            .arena
            .source_files
            .first()
            .and_then(|file| file.text.get(start as usize..end as usize))
        else {
            return end;
        };
        let trimmed = text.trim_end_matches([';', ',', ' ', '\t', '\r', '\n']);
        start + trimmed.len() as u32
    }

    /// Whether the source between two offsets contains a line break — the
    /// offset-level form of `tsc`'s `startLine < endLine` comparison.
    fn span_spans_multiple_lines(&self, start: u32, end: u32) -> bool {
        self.ctx
            .arena
            .source_files
            .first()
            .and_then(|file| file.text.get(start as usize..end as usize))
            .is_some_and(|text| text.contains('\n'))
    }
}
