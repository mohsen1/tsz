//! Function return-type guard helpers for declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn array_element_type_text(
        type_text: &str,
    ) -> Option<String> {
        let trimmed = type_text.trim();
        if let Some(element) = trimmed.strip_suffix("[]") {
            let element = element.trim();
            if !element.is_empty() {
                return Some(element.to_string());
            }
        }
        for prefix in ["Array<", "ReadonlyArray<"] {
            if let Some(inner) = trimmed
                .strip_prefix(prefix)
                .and_then(|text| text.strip_suffix('>'))
            {
                let inner = inner.trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
        }
        None
    }

    /// Returns `true` when `func_body` is a block whose sole non-trivial
    /// return expression is an object literal that contains at least one
    /// method whose body only returns `this`.
    ///
    /// When this is true, the solver infers a recursive "self-referential"
    /// object type for the function.  Printing that type through the solver's
    /// `TypePrinter` (with `max_depth = 128`) produces an exponentially large
    /// string.  The AST-based path already handles these methods correctly by
    /// emitting `/*elided*/ any` for the `this`-returning slots, so we prefer
    /// the source-derived type text and skip the expensive solver print.
    ///
    /// This is intentionally conservative: it only matches single-return
    /// functions whose return is a direct object literal.  More complex shapes
    /// (multiple returns, nested wrappers, etc.) fall through to the normal
    /// solver path.
    pub(in crate::declaration_emitter) fn function_body_returns_object_with_this_only_methods(
        &self,
        func_body: NodeIndex,
    ) -> bool {
        let body_node = match self.arena.get(func_body) {
            Some(n) => n,
            None => return false,
        };
        let block = match self.arena.get_block(body_node) {
            Some(b) => b,
            None => return false,
        };

        // Collect all non-trivial statements; we expect exactly one return.
        let returns: Vec<_> = block
            .statements
            .nodes
            .iter()
            .copied()
            .filter_map(|stmt_idx| {
                let stmt_node = self.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                let ret = self.arena.get_return_statement(stmt_node)?;
                ret.expression.is_some().then_some(ret.expression)
            })
            .collect();

        if returns.len() != 1 {
            return false;
        }

        let expr_idx = returns[0];
        let expr_node = match self.arena.get(expr_idx) {
            Some(n) => n,
            None => return false,
        };
        if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let obj = match self.arena.get_literal_expr(expr_node) {
            Some(o) => o,
            None => return false,
        };

        obj.elements.nodes.iter().copied().any(|prop_idx| {
            let prop_node = match self.arena.get(prop_idx) {
                Some(n) => n,
                None => return false,
            };
            let method = match self.arena.get_method_decl(prop_node) {
                Some(m) => m,
                None => return false,
            };
            self.method_body_returns_this(method.body)
        })
    }
}
