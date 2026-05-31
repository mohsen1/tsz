use super::IRPrinter;
use crate::transforms::ir::IRNode;
use tsz_parser::syntax_kind_ext;

impl<'a> IRPrinter<'a> {
    pub(super) fn is_recovered_empty_property_access(&self, node: &IRNode) -> bool {
        match node {
            IRNode::PropertyAccess { property, .. } => property.is_empty(),
            IRNode::Raw(text) => text.trim_end().ends_with('.'),
            IRNode::ASTRef(idx) => self.arena.is_some_and(|arena| {
                arena.get(*idx).is_some_and(|node| {
                    if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        && arena.get_access_expr(node).is_some_and(|access| {
                            arena.is_missing_recovery_identifier(access.name_or_argument)
                        })
                    {
                        return true;
                    }

                    self.source_text.is_some_and(|source_text| {
                        let start = (node.pos as usize).min(source_text.len());
                        let end = (node.end as usize).min(source_text.len());
                        if start < end {
                            let span = &source_text[start..end];
                            span.trim_end().ends_with('.')
                                || Self::source_span_has_recovered_property_boundary(span)
                                || Self::source_has_recovered_property_boundary(source_text, end)
                        } else {
                            Self::source_has_recovered_property_boundary(source_text, end)
                        }
                    })
                })
            }),
            _ => false,
        }
    }

    fn source_has_recovered_property_boundary(source_text: &str, end: usize) -> bool {
        let bytes = source_text.as_bytes();
        if bytes.get(end) != Some(&b'.') {
            return false;
        }

        let mut i = end + 1;
        while let Some(byte) = bytes.get(i) {
            match byte {
                b' ' | b'\t' | b'\r' => i += 1,
                b'\n' | b';' | b'}' => return true,
                _ => return false,
            }
        }
        true
    }

    fn source_span_has_recovered_property_boundary(span: &str) -> bool {
        let bytes = span.as_bytes();
        for dot in bytes
            .iter()
            .enumerate()
            .filter_map(|(i, byte)| (*byte == b'.').then_some(i))
        {
            let mut i = dot + 1;
            while let Some(byte) = bytes.get(i) {
                match byte {
                    b' ' | b'\t' | b'\r' => i += 1,
                    b'\n' => {
                        i += 1;
                        while matches!(bytes.get(i), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                            i += 1;
                        }
                        return bytes.get(i).is_none_or(|byte| matches!(byte, b';' | b'}'));
                    }
                    b';' | b'}' => return true,
                    _ => break,
                }
            }
        }
        false
    }
}
