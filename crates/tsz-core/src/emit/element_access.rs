use crate::syntax::Expression;

use super::{PREC_LOWEST, PREC_POSTFIX, Printer};

impl Printer<'_> {
    pub(super) fn write_element_access(&mut self, object: &Expression, index: &Expression) {
        self.write_expression(object, PREC_POSTFIX);
        self.write_comments_through_token(object.span.end);
        self.output.push('[');
        self.write_expression(index, PREC_LOWEST);
        self.write_comments_through_token(index.span.end);
        self.output.push(']');
    }
}
