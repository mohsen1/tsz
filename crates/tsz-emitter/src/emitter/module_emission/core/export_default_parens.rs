use super::super::super::Printer;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> Printer<'a> {
    /// True when `export default <expr>` has source `(<class|fn> as T)` or
    /// equivalent. The parens only existed to delimit the type cast, but
    /// stripping them after type erasure would change the export semantics
    /// from "default-export an expression" to "default-export a declaration".
    pub(super) fn export_default_paren_protects_class_or_function(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return false;
        };
        let Some(inner) = self.arena.get(paren.expression) else {
            return false;
        };
        if inner.kind != syntax_kind_ext::AS_EXPRESSION
            && inner.kind != syntax_kind_ext::SATISFIES_EXPRESSION
            && inner.kind != syntax_kind_ext::TYPE_ASSERTION
        {
            return false;
        }
        let Some(assertion) = self.arena.get_type_assertion(inner) else {
            return false;
        };
        let Some(operand) = self.arena.get(assertion.expression) else {
            return false;
        };
        operand.kind == syntax_kind_ext::CLASS_EXPRESSION
            || operand.kind == syntax_kind_ext::FUNCTION_EXPRESSION
    }
}
