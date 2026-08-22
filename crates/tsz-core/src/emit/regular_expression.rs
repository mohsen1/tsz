use crate::syntax::RegularExpressionLiteral;

use super::Printer;

impl Printer<'_> {
    pub(super) fn write_regular_expression(&mut self, literal: &RegularExpressionLiteral) {
        self.output.push_str(&literal.raw);
    }
}
