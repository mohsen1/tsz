use super::super::{JsxEmit, ModuleKind, Printer};

use super::{
    AttrGroup, JsxAttrInfo, JsxAttrValue, JsxAttrsInfo, JsxChildSep, decode_jsx_entities,
    escape_jsx_text_for_js_with_quote, needs_quoting, process_jsx_text,
};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::Node;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

fn skip_jsx_quoted_text(bytes: &[u8], mut pos: usize, end: usize) -> usize {
    let quote = bytes[pos];
    pos += 1;
    while pos < end {
        if bytes[pos] == b'\\' {
            pos = (pos + 2).min(end);
        } else if bytes[pos] == quote {
            return pos + 1;
        } else {
            pos += 1;
        }
    }
    end
}

fn skip_jsx_braced_expression(bytes: &[u8], mut pos: usize, end: usize) -> usize {
    let mut depth = 1usize;
    pos += 1;
    while pos < end {
        match bytes[pos] {
            b'\'' | b'"' | b'`' => {
                pos = skip_jsx_quoted_text(bytes, pos, end);
            }
            b'/' if pos + 1 < end && bytes[pos + 1] == b'/' => {
                pos += 2;
                while pos < end && !matches!(bytes[pos], b'\n' | b'\r') {
                    pos += 1;
                }
            }
            b'/' if pos + 1 < end && bytes[pos + 1] == b'*' => {
                pos += 2;
                while pos + 1 < end {
                    if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                        pos += 2;
                        break;
                    }
                    pos += 1;
                }
            }
            b'{' => {
                depth += 1;
                pos += 1;
            }
            b'}' => {
                depth -= 1;
                pos += 1;
                if depth == 0 {
                    return pos;
                }
            }
            _ => {
                pos += 1;
            }
        }
    }
    end
}

include!("emit_parts/part1.rs");
include!("emit_parts/part2.rs");
