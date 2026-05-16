use super::Printer;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn has_recovered_accessor_modifier(&self, node: &Node) -> bool {
        let Some(source_text) = self.source_text else {
            return false;
        };
        let mut end = std::cmp::min(node.pos as usize, source_text.len());
        let bytes = source_text.as_bytes();

        while end > 0 && matches!(bytes[end - 1], b' ' | b'\t') {
            end -= 1;
        }
        if end == 0 || matches!(bytes[end - 1], b'\n' | b'\r') {
            return false;
        }

        let mut start = end;
        while start > 0 && bytes[start - 1].is_ascii_alphabetic() {
            start -= 1;
        }

        &source_text[start..end] == "accessor"
    }
}
