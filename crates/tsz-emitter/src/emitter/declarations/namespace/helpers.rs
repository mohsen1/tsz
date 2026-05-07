use crate::transforms::ir::IRNode;

/// Rewrite enum IIFE IR from `E || (E = {})` to `E = NS.E || (NS.E = {})`
/// for exported enums in namespaces.
pub(in crate::emitter) fn rewrite_enum_iife_for_namespace_export(
    ir: &mut IRNode,
    enum_name: &str,
    ns_name: &str,
) {
    // The IR from EnumES5Transformer is:
    //   Sequence([VarDecl { name }, ExpressionStatement(CallExpr { callee, arguments: [iife_arg] })])
    // where iife_arg is: LogicalOr { left: Identifier(E), right: BinaryExpr(E = {}) }
    //
    // We need to transform it to:
    //   iife_arg = BinaryExpr(E = LogicalOr { left: NS.E, right: BinaryExpr(NS.E = {}) })
    let IRNode::Sequence(stmts) = ir else {
        return;
    };

    // Find the ExpressionStatement containing the CallExpr.
    let Some(expr_stmt) = stmts.iter_mut().find_map(|s| match s {
        IRNode::ExpressionStatement(inner) => Some(inner),
        _ => None,
    }) else {
        return;
    };

    let IRNode::CallExpr { arguments, .. } = expr_stmt.as_mut() else {
        return;
    };

    if arguments.len() != 1 {
        return;
    }

    let ns_prop = || IRNode::PropertyAccess {
        object: Box::new(IRNode::Identifier(ns_name.to_string().into())),
        property: enum_name.to_string().into(),
    };

    // Replace the IIFE argument: E || (E = {}) -> E = NS.E || (NS.E = {}).
    arguments[0] = IRNode::BinaryExpr {
        left: Box::new(IRNode::Identifier(enum_name.to_string().into())),
        operator: "=".to_string().into(),
        right: Box::new(IRNode::LogicalOr {
            left: Box::new(ns_prop()),
            right: Box::new(IRNode::BinaryExpr {
                left: Box::new(ns_prop()),
                operator: "=".to_string().into(),
                right: Box::new(IRNode::empty_object()),
            }),
        }),
    };
}

pub(super) fn find_unescaped_template_end(source: &str, template_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pos = template_start.checked_add(1)?;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'`' {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn skip_quoted_source_text(source: &str, quote_start: usize) -> usize {
    let bytes = source.as_bytes();
    let quote = bytes[quote_start];
    if quote == b'`' {
        return find_unescaped_template_end(source, quote_start)
            .map(|end| end + 1)
            .unwrap_or(source.len());
    }

    let mut pos = quote_start + 1;
    while pos < bytes.len() {
        if bytes[pos] == b'\\' {
            pos += 2;
            continue;
        }
        if bytes[pos] == quote {
            return pos + 1;
        }
        pos += 1;
    }
    source.len()
}

pub(super) fn find_next_code_module_keyword(source: &str, mut cursor: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                cursor += 2;
                while cursor < bytes.len() && !matches!(bytes[cursor], b'\n' | b'\r') {
                    cursor += 1;
                }
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor += 2;
                while cursor + 1 < bytes.len()
                    && !(bytes[cursor] == b'*' && bytes[cursor + 1] == b'/')
                {
                    cursor += 1;
                }
                cursor = (cursor + 2).min(bytes.len());
            }
            b'\'' | b'"' | b'`' => {
                cursor = skip_quoted_source_text(source, cursor);
            }
            b'm' if source[cursor..].starts_with("module")
                && cursor
                    .checked_sub(1)
                    .and_then(|prev| bytes.get(prev))
                    .is_none_or(|byte| {
                        !byte.is_ascii_alphanumeric() && *byte != b'_' && *byte != b'$'
                    })
                && bytes.get(cursor + "module".len()).is_none_or(|byte| {
                    !byte.is_ascii_alphanumeric() && *byte != b'_' && *byte != b'$'
                }) =>
            {
                return Some(cursor);
            }
            _ => {
                cursor += source[cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
            }
        }
    }
    None
}
