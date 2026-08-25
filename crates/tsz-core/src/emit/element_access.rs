use crate::syntax::Expression;

use super::{PREC_LOWEST, PREC_POSTFIX, Printer};

impl Printer<'_> {
    pub(super) fn write_member_access(&mut self, object: &Expression, name: &str) {
        self.write_member_object(object);
        self.output.push('.');
        self.output.push_str(name);
    }

    pub(super) fn write_element_access(&mut self, object: &Expression, index: &Expression) {
        self.write_expression(object, PREC_POSTFIX);
        if self.write_comments_through_token(object.span.end) {
            self.write_indent();
        }
        self.output.push('[');
        self.write_expression(index, PREC_LOWEST);
        if self.write_comments_through_token(index.span.end) {
            self.write_indent();
        }
        self.output.push(']');
    }
}
