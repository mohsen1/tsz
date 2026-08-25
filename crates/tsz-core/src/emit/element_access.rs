use crate::source::Span;
use crate::syntax::{Expression, TokenKind};

use super::{End, Gap, Kind, PREC_LOWEST, PREC_POSTFIX, Printer};

impl Printer<'_> {
    pub(super) fn write_member_access(&mut self, object: &Expression, name: &str, name_span: Span) {
        self.write_member_object(object);
        self.indent += 1;
        let (_, broke_line) = self.write_gap(End(object.span.end), true, Gap::Indent);
        self.output.push('.');
        self.indent += usize::from(broke_line);
        self.write_gap(Kind(TokenKind::Dot, name_span.start), true, Gap::Indent);
        self.output.push_str(name);
        self.indent = self.indent.saturating_sub(1 + usize::from(broke_line));
    }

    pub(super) fn write_element_access(&mut self, object: &Expression, index: &Expression) {
        self.write_expression(object, PREC_POSTFIX);
        self.write_gap(End(object.span.end), true, Gap::Indent);
        self.output.push('[');
        self.write_expression(index, PREC_LOWEST);
        self.write_gap(End(index.span.end), true, Gap::Indent);
        self.output.push(']');
    }
}
